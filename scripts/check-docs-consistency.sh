#!/usr/bin/env bash
# =============================================================================
# Limiteron 文档一致性核对脚本
# =============================================================================
#
# 核对文档中引用的仓库内路径、feature 名、关键 API 符号与示例清单是否与
# 源码 / Cargo.toml 真实一致，防止文档漂移（文档描述的文件/能力已不存在）。
#
# 使用方法:
#   ./scripts/check-docs-consistency.sh
#
# 检查项目:
#   1. 文档中引用的仓库内路径（行内代码与 Markdown 链接）真实存在
#   2. 文档中引用的 feature 名已在 Cargo.toml 定义（limiteron / examples / macros）
#   3. 文档中与源码路径关联引用的 API 符号在 src/ 中真实存在
#   4. examples/README.md 示例清单与 examples/src/bin/ 目录双向一致
#   5. src/AGENTS.md 模块表与 src/ 实际结构一致（该文件不入库，仅本地存在时检查）
#
# 排除文档：docs/CHANGELOG.md（历史版本记录）、docs/COVERAGE_REPORT.md
# （历史覆盖率快照）——其中的路径与 feature 反映的是当时而非当前仓库状态。
#
# =============================================================================

# shellcheck disable=SC2016  # grep/sed ERE 中的反引号是字面量，非命令替换

set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

VIOLATIONS=0

log_section() {
    echo ""
    echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  $1${NC}"
    echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
}

fail() {
    echo -e "${RED}  ✗ $1${NC}"
    VIOLATIONS=$((VIOLATIONS + 1))
}

pass() {
    echo -e "${GREEN}  ✓ $1${NC}"
}

# 参与核对的文档集合（历史性文档除外，理由见文件头注释；
# AGENTS.md 系列 .gitignore 忽略不入库，CI checkout 后不存在，按实际存在裁剪）
DOCS=()
for doc in README.md README_EN.md AGENTS.md src/AGENTS.md examples/README.md docs/*.md; do
    [[ "$doc" == docs/CHANGELOG.md || "$doc" == docs/COVERAGE_REPORT.md ]] && continue
    [[ -f "$doc" ]] && DOCS+=("$doc")
done

# =============================================================================
# 1. 仓库内路径存在性（行内代码 + Markdown 链接）
# =============================================================================

check_paths() {
    log_section "检查 1/5：文档引用的仓库内路径"

    # 路径首段必须是仓库根真实存在的目录/文件才视为路径引用，
    # 避免把普通名词误判为路径；src/main.rs 是教程中用户项目的惯用路径，跳过
    for doc in "${DOCS[@]}"; do
        doc_dir="$(dirname "$doc")"

        while IFS=$'\t' read -r line token; do
            rel="${token#\`}"
            rel="${rel%\`}"
            while [[ "$rel" == ../* || "$rel" == ./* ]]; do
                if [[ "$rel" == ../* ]]; then
                    doc_dir="$(dirname "$doc_dir")"
                fi
                rel="${rel#*/}"
            done
            [[ -z "$rel" || "$rel" == */ || "$rel" == *//* ]] && continue
            [[ "$rel" == "src/main.rs" ]] && continue
            first="${rel%%/*}"
            [[ -d "$first" || -e "$first" ]] || continue
            if [[ ! -e "$rel" ]]; then
                fail "路径不存在: $doc:$line 引用 $token"
            fi
        done < <(grep -noE '`[A-Za-z0-9_][A-Za-z0-9_./-]*`' "$doc" | sed -E 's|^([0-9]+):|\1\t|' || true)

        while IFS=$'\t' read -r line target; do
            target="${target%%\#*}"
            target="${target%% *}"
            target="${target%\"}"
            [[ -z "$target" ]] && continue
            [[ "$target" == http:* || "$target" == https:* || "$target" == mailto:* ]] && continue
            # 越出仓库根的相对链接（如 GitHub 页面的 ../../issues）不属于文件系统路径
            depth=0
            [[ "$doc_dir" != "." ]] && depth="$(awk -F/ '{print NF}' <<<"$doc_dir")"
            ups=0
            probe="$target"
            while [[ "$probe" == ../* ]]; do ups=$((ups + 1)); probe="${probe#../}"; done
            [[ "$ups" -gt "$depth" ]] && continue
            resolved="$doc_dir/$target"
            if [[ ! -e "$resolved" ]]; then
                fail "链接目标不存在: $doc:$line → $target"
            fi
        done < <(grep -noE '\]\([^)]+\)' "$doc" | sed -E 's|^([0-9]+):\]\((.*)\)$|\1\t\2|' || true)
    done
    pass "路径核对完成"
}

# =============================================================================
# 2. feature 名与 Cargo.toml 定义一致
# =============================================================================

collect_cargo_features() {
    awk '/^\[features\]/{f=1; next} /^\[/{f=0} f && /^[a-z][a-z0-9_-]*[[:space:]]*=/ {
        sub(/[[:space:]]*=.*/, ""); gsub(/[[:space:]]/, ""); print
    }' "$1"
}

check_features() {
    log_section "检查 2/5：文档引用的 feature 名"

    local feature_set
    feature_set="$(collect_cargo_features Cargo.toml; collect_cargo_features examples/Cargo.toml; collect_cargo_features macros/Cargo.toml)"
    feature_set="$(sort -u <<<"$feature_set")"

    local mismatch=0
    while IFS=$'\t' read -r token locations; do
        if ! grep -qxF "$token" <<<"$feature_set"; then
            fail "未定义的 feature: \`$token\` 引用于 $locations"
            mismatch=1
        fi
    done < <(sort -t$'\t' -k1,1 "$1" | awk -F'\t' '{if ($1 == prev) {locs = locs ", " $2} else {if (NR > 1) print prev "\t" locs; prev = $1; locs = $2}} END {if (NR >= 1) print prev "\t" locs}')

    if [[ "$mismatch" -eq 0 ]]; then pass "所有引用的 feature 均已定义"; fi
}

gather_feature_refs() {
    local out="$1"
    : >"$out"

    for doc in "${DOCS[@]}"; do
        # `name` 特性 / `name` feature（跳过"已移除/已弃用"历史说明语境中的引用）
        grep -noE '.*`[a-z][a-z0-9-]+` (特性|feature).*' "$doc" 2>/dev/null \
            | grep -viE '移除|删除|弃用|废弃|removed|deprecated' \
            | sed -E 's#^([0-9]+):.*`([a-z][a-z0-9-]+)` (特性|feature).*#\2\t'"$doc"':\1#' >>"$out" || true
        # --features a,b / --features "a, b"
        grep -noE -- '--features[= ]("[^"]+"|[a-z0-9-,]+)' "$doc" 2>/dev/null \
            | sed -E 's|^([0-9]+):.*--features[= ]"?([^"]*)"?.*|\1\t'"$doc"':\2|' \
            | awk -F'\t' '{n = split($2, a, ","); for (i = 1; i <= n; i++) print a[i] "\t" $1}' >>"$out" || true
    done
    # features = [...] 的名字拆分（保留文件信息需逐条处理，此处单独展开）
    for doc in "${DOCS[@]}"; do
        grep -noE 'features *= *\[[^]]*\]' "$doc" 2>/dev/null | while IFS=: read -r line block; do
            sed -E 's/.*\[//; s/\]//; s/"//g' <<<"$block" | tr ',' '\n' | sed -E 's/^[[:space:]]+|[[:space:]]+$//' \
                | grep -E '^[a-z][a-z0-9-]*$' | sed "s|^\(.*\)$|\1\t$doc:$line|" >>"$out" || true
        done || true
    done
    # README feature 表首列（<td><code>name</code></td>）
    for doc in README.md README_EN.md; do
        grep -noE '<td><code>[a-z][a-z0-9-]+</code></td>' "$doc" 2>/dev/null \
            | sed -E 's/^([0-9]+):<td><code>([a-z0-9-]+)<\/code><\/td>/\2\t'"$doc"':\1/' >>"$out" || true
    done
    grep -vE '^[[:space:]]*$' "$out" | sort -u -t$'\t' -k1,1 >"$out.sorted" && mv "$out.sorted" "$out"
}

# =============================================================================
# 3. 关键 API 符号存在性
# =============================================================================

check_api_symbols() {
    log_section "检查 3/5：关键 API 符号"

    local missing=0

    # `src/path` 与紧随其后的 `symbol` 构成关联引用，symbol 必须在 src/ 中出现
    while IFS=$'\t' read -r doc line path symbol; do
        if ! grep -rqw "$symbol" src/; then
            fail "API 符号不存在: $doc:$line \`$symbol\`（关联路径 $path）"
            missing=1
        fi
    done < <(grep -rnoE '`src/[A-Za-z0-9_/.-]+` 的 `[A-Za-z0-9_]+`' "${DOCS[@]}" 2>/dev/null \
        | sed -E 's/^([^:]+):([0-9]+):`([^`]+)` 的 `([A-Za-z0-9_]+)`/\1\t\2\t\3\t\4/')

    # `Type::item` 限定符号：item 名须在 src/ 中出现（定义或调用）
    while IFS=$'\t' read -r doc line qualified; do
        local item="${qualified##*::}"
        item="${item%\`}"
        if ! grep -rqw "$item" src/; then
            fail "API 符号不存在: $doc:$line $qualified"
            missing=1
        fi
    done < <(grep -rnoE '`[A-Z][A-Za-z0-9]+::[a-z_]\w+`' "${DOCS[@]}" 2>/dev/null \
        | sed -E 's/^([^:]+):([0-9]+):(.*)/\1\t\2\t\3/')

    if [[ "$missing" -eq 0 ]]; then pass "所有关联引用的 API 符号均存在"; fi
}

# =============================================================================
# 4. examples 清单与 examples/src/bin/ 目录一致
# =============================================================================

check_examples() {
    log_section "检查 4/5：examples 示例清单"

    local doc_bins actual_bins
    doc_bins="$(grep -oE -- '--bin [a-z0-9_]+' examples/README.md | awk '{print $2}' | sort -u)"
    actual_bins="$(find examples/src/bin -maxdepth 1 -name '*.rs' -exec basename {} .rs \; | sort -u)"

    local drift=0
    while IFS= read -r bin; do
        [[ -z "$bin" ]] && continue
        grep -qxF "$bin" <<<"$actual_bins" || { fail "清单中的示例不存在: examples/src/bin/$bin.rs"; drift=1; }
    done <<<"$doc_bins"
    while IFS= read -r bin; do
        [[ -z "$bin" ]] && continue
        grep -qxF "$bin" <<<"$doc_bins" || { fail "示例未列入清单: $bin.rs 存在但 examples/README.md 未记载"; drift=1; }
    done <<<"$actual_bins"

    # 文档声明的示例数量必须与实际数量一致
    local count
    count="$(find examples/src/bin -maxdepth 1 -name '*.rs' | wc -l)"
    for doc in examples/README.md README.md README_EN.md; do
        while IFS= read -r declared; do
            [[ "$declared" == "$count" ]] || fail "示例数量声明不符: $doc 声明 $declared 个，实际 $count 个"
        done < <(grep -oE '[0-9]+ 个可运行示例|[0-9]+ runnable examples' "$doc" 2>/dev/null \
            | grep -oE '^[0-9]+' | sort -u)
    done

    if [[ "$drift" -eq 0 ]]; then pass "示例清单与目录双向一致（$count 个）"; fi
}

# =============================================================================
# 5. src/AGENTS.md 模块表与 src/ 实际结构一致
# =============================================================================

check_src_agents() {
    log_section "检查 5/5：src/AGENTS.md 模块表"

    if [[ ! -f src/AGENTS.md ]]; then
        pass "跳过 src/AGENTS.md（.gitignore 忽略的本地导航文件，CI 环境不存在）"
        return 0
    fi

    local drift=0
    while IFS= read -r token; do
        [[ -z "$token" ]] && continue
        if [[ "$token" == */ ]]; then
            # 裸目录名允许命中任一子模块（"integrations/ (kit/: ...)" 的 kit/
            # 指 integrations/kit/ 而非 src/kit/）
            if [[ ! -d "src/${token%/}" ]] && ! compgen -G "src/*/${token%/}" >/dev/null; then
                fail "src/AGENTS.md 引用的模块目录不存在: src/${token%/}"
                drift=1
            fi
        else
            # 裸文件名允许命中任一子模块（表格中 "telemetry/ (mod.rs, ...)" 的
            # mod.rs 指 telemetry/mod.rs 而非 src/mod.rs）
            if [[ ! -f "src/$token" ]] && ! compgen -G "src/*/$token" >/dev/null; then
                fail "src/AGENTS.md 引用的源文件不存在: src/$token"
                drift=1
            fi
        fi
    done < <(grep -E '^\|' src/AGENTS.md \
        | grep -oE '[A-Za-z_][A-Za-z0-9_./]*' \
        | grep -E '^[a-z_][a-z0-9_]*(/[a-z_][a-z0-9_]*)*(\.rs|/)$' | sort -u)

    if [[ "$drift" -eq 0 ]]; then pass "src/AGENTS.md 模块表与实际结构一致"; fi
}

# =============================================================================
# 主流程
# =============================================================================

log_section "Limiteron 文档一致性核对"

FEATURE_REFS="$(mktemp)"
trap 'rm -f "$FEATURE_REFS"' EXIT
gather_feature_refs "$FEATURE_REFS"

check_paths
check_features "$FEATURE_REFS"
check_api_symbols
check_examples
check_src_agents

echo ""
if [[ "$VIOLATIONS" -gt 0 ]]; then
    echo -e "${RED}文档一致性核对失败：$VIOLATIONS 处漂移${NC}"
    exit 1
fi
echo -e "${GREEN}文档一致性核对通过${NC}"
