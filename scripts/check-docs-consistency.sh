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
#   （并接入 lefthook.yml / .pre-commit-config.yaml 本地 pre-commit 与 CI doc job）
#
# 检查项目:
#   1. 文档中引用的仓库根相对路径（行内代码与 Markdown 链接）真实存在。
#      行内代码正则不含点首字符，`../x` 形态的行内码不在核对范围；
#      相对路径由 Markdown 链接检查覆盖
#   2. 文档中引用的 feature 名已在 Cargo.toml 定义（limiteron / examples / macros）
#   3. 文档中与源码路径关联引用的 API 符号在 src/ 中真实存在
#   4. examples/README.md 示例清单与 examples/src/bin/ 目录双向一致
#   5. src/AGENTS.md 模块表与 src/ 实际结构一致（该文件不入库，仅本地存在时检查；
#      本地 pre-commit 钩子是其主要执行点，CI 环境自动跳过并显式提示）
#
# 排除文档：docs/CHANGELOG.md（历史版本记录）、docs/COVERAGE_REPORT.md
# （历史覆盖率快照）——其中的路径与 feature 反映的是当时而非当前仓库状态。
#
# =============================================================================

# shellcheck disable=SC2016  # grep/sed/awk ERE 中的反引号是字面量，非命令替换

set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

VIOLATIONS=0

# 消息以 %s 输出，文档派生内容中的反斜杠序列不被解释为转义/ANSI
log_section() {
    printf '\n%s\n' "$CYAN═══════════════════════════════════════════════════════════════$NC"
    printf '%s\n' "$CYAN  $1$NC"
    printf '%s\n' "$CYAN═══════════════════════════════════════════════════════════════$NC"
}

fail() {
    printf '%s  ✗ %s%s\n' "$RED" "$1" "$NC"
    VIOLATIONS=$((VIOLATIONS + 1))
}

pass() {
    printf '%s  ✓ %s%s\n' "$GREEN" "$1" "$NC"
}

# 提示级问题：不计入失败，仅保证静默退化在日志中可见
warn() {
    printf '%s  ⚠ %s%s\n' "$YELLOW" "$1" "$NC"
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

    # 首段必须是仓库根真实存在的目录/文件才视为路径引用（防普通名词误报），
    # 首段不存在的复合形态引用记入 SKIP 清单，在脚本末尾以 warn 级输出供人工抽查；
    # src/main.rs 是教程中用户项目的惯用路径，跳过
    local tops
    tops="$(printf '%s\n' * .[!.]*)"

    local bad=0
    : >"$SKIPPED_REFS"
    for doc in "${DOCS[@]}"; do
        doc_dir="$(dirname "$doc")"

        while IFS=$'\t' read -r kind token line; do
            case "$kind" in
                CAND)
                    if [[ ! -e "$token" ]]; then
                        fail "路径不存在: $doc:$line 引用 \`$token\`"
                        bad=1
                    fi
                    ;;
                SKIP)
                    printf '%s:%s\t%s\n' "$doc" "$line" "$token" >>"$SKIPPED_REFS"
                    ;;
            esac
        done < <(awk -v tops="$tops" '
            BEGIN { n = split(tops, arr, "\n"); for (i = 1; i <= n; i++) top[arr[i]] = 1 }
            {
                s = $0
                while (match(s, /`[A-Za-z0-9_][A-Za-z0-9_./-]*`/)) {
                    token = substr(s, RSTART + 1, RLENGTH - 2)
                    s = substr(s, RSTART + RLENGTH)
                    if (token == "src/main.rs" || token ~ /\/\//) continue
                    split(token, seg, "/")
                    if (seg[1] in top) print "CAND\t" token "\t" NR
                    else if (token ~ /[\/.]/) print "SKIP\t" token "\t" NR
                }
            }' "$doc" || true)

        # Markdown 链接（相对目标基于所在文档目录解析）
        slashes="${doc_dir//[^\/]/}"
        depth=${#slashes}
        while IFS=$'\t' read -r line target; do
            target="${target%%\#*}"
            target="${target%% *}"
            target="${target%\"}"
            [[ -z "$target" ]] && continue
            [[ "$target" == http:* || "$target" == https:* || "$target" == mailto:* ]] && continue
            # 越出仓库根的相对链接（如 GitHub 页面的 ../../issues）不属于文件系统路径
            ups=0
            probe="$target"
            while [[ "$probe" == ../* ]]; do ups=$((ups + 1)); probe="${probe#../}"; done
            [[ "$ups" -gt "$depth" ]] && continue
            if [[ ! -e "$doc_dir/$target" ]]; then
                fail "链接目标不存在: $doc:$line → $target"
                bad=1
            fi
        done < <(grep -noE '\]\([^)]+\)' "$doc" | sed -E 's|^([0-9]+):\]\((.*)\)$|\1\t\2|' || true)
    done

    if [[ "$bad" -eq 0 ]]; then pass "路径核对完成"; fi
}

# =============================================================================
# 2. feature 名与 Cargo.toml 定义一致
# =============================================================================

collect_cargo_features() {
    [[ -f "$1" ]] || return 0
    awk '/^\[features\]/{f=1; next} /^\[/{f=0} f && /^[a-z][a-z0-9_-]*[[:space:]]*=/ {
        sub(/[[:space:]]*=.*/, ""); gsub(/[[:space:]]/, ""); print
    }' "$1"
}

check_features() {
    log_section "检查 2/5：文档引用的 feature 名"

    # 文档措辞改版可能使三类引用模式全部失配：空集必须显式可见而非静默通过
    if [[ ! -s "$1" ]]; then
        warn "0 条 feature 引用被核对（未匹配到任何引用模式，请确认文档措辞未漂移）"
        return 0
    fi

    local feature_set
    feature_set="$(collect_cargo_features Cargo.toml; collect_cargo_features examples/Cargo.toml; collect_cargo_features macros/Cargo.toml)" || true
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
        # `name` 特性 / `name` feature：逐个提取（一行可含多个引用），
        # 跳过"已移除/已弃用"历史说明语境中的引用
        awk -v doc="$doc" '
            tolower($0) ~ /(移除|删除|弃用|废弃|removed|deprecated)/ { next }
            {
                s = $0
                while (match(s, /`[a-z][a-z0-9_-]+` (特性|feature)/)) {
                    seg = substr(s, RSTART, RLENGTH)
                    sub(/^`/, "", seg)
                    sub(/` (特性|feature)$/, "", seg)
                    print seg "\t" doc ":" NR
                    s = substr(s, RSTART + RLENGTH)
                }
            }' "$doc" >>"$out" || true
        # --features a,b / --features "a, b"
        grep -noE -- '--features[= ]("[^"]+"|[a-z0-9_,-]+)' "$doc" 2>/dev/null \
            | sed -E 's|^([0-9]+):.*--features[= ]"?([^"]*)"?.*|'"$doc"':\1\t\2|' \
            | awk -F'\t' '{n = split($2, a, ","); for (i = 1; i <= n; i++) {g = a[i]; gsub(/^[[:space:]]+|[[:space:]]+$/, "", g); if (g != "") print g "\t" $1}}' >>"$out" || true
        # toml 代码块中的 features = [...]（单条 awk 完成提取与拆分）
        awk -v doc="$doc" '
            match($0, /features *= *\[[^]]*\]/) {
                s = substr($0, RSTART, RLENGTH)
                sub(/.*\[/, "", s); sub(/\].*/, "", s); gsub(/"/, "", s)
                n = split(s, a, ",")
                for (i = 1; i <= n; i++) {
                    gsub(/^[ \t]+|[ \t]+$/, "", a[i])
                    if (a[i] ~ /^[a-z][a-z0-9_-]*$/) print a[i] "\t" doc ":" NR
                }
            }' "$doc" >>"$out" || true
    done
    # README feature 表首列（<td><code>name</code></td>）
    for doc in README.md README_EN.md; do
        grep -noE '<td><code>[a-z][a-z0-9_-]+</code></td>' "$doc" 2>/dev/null \
            | sed -E 's/^([0-9]+):<td><code>([a-z0-9_-]+)<\/code><\/td>/\2\t'"$doc"':\1/' >>"$out" || true
    done
    # 保留重复行（同一 feature 的多个引用位置），位置聚合由 check_features 的 awk 完成
    sort -t$'\t' -k1,1 -o "$out" "$out" || true
}

# =============================================================================
# 3. 关键 API 符号存在性
# =============================================================================

check_api_symbols() {
    local symbols_file="$1"
    log_section "检查 3/5：关键 API 符号"

    # 一次性构建 src/ 标识符词表，逐符号查词表而非逐符号全树递归扫描
    grep -rhoE '[A-Za-z_][A-Za-z0-9_]*' src/ 2>/dev/null | sort -u >"$symbols_file" || true

    local missing=0

    # `src/path` 与紧随其后的 `symbol` 构成关联引用，symbol 必须在 src/ 中出现
    while IFS=$'\t' read -r doc line path symbol; do
        if ! grep -qxF "$symbol" "$symbols_file"; then
            fail "API 符号不存在: $doc:$line \`$symbol\`（关联路径 $path）"
            missing=1
        fi
    done < <(grep -rnoE '`src/[A-Za-z0-9_/.-]+` 的 `[A-Za-z0-9_]+`' "${DOCS[@]}" 2>/dev/null \
        | sed -E 's/^([^:]+):([0-9]+):`([^`]+)` 的 `([A-Za-z0-9_]+)`/\1\t\2\t\3\t\4/')

    # `Type::item` 限定符号：item 名须在 src/ 中出现（定义或调用）
    while IFS=$'\t' read -r doc line qualified; do
        local item="${qualified##*::}"
        item="${item%\`}"
        if ! grep -qxF "$item" "$symbols_file"; then
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

    # 关键前提缺失时显式失败，避免双向清单比对在空集上静默通过
    local premise=0
    if [[ ! -f examples/README.md ]]; then
        fail "examples/README.md 不存在，示例清单无从核对"
        premise=1
    fi
    if [[ ! -d examples/src/bin ]]; then
        fail "examples/src/bin 目录不存在，示例无从核对"
        premise=1
    fi
    if [[ "$premise" -eq 1 ]]; then return 0; fi

    local doc_bins actual_bins
    doc_bins="$(awk 'match($0, /--bin [a-z0-9_]+/) { print substr($0, RSTART + 6, RLENGTH - 6) }' examples/README.md | sort -u)"
    actual_bins=""
    local count=0
    for f in examples/src/bin/*.rs; do
        [[ -e "$f" ]] || continue
        b="${f##*/}"
        actual_bins+="${b%.rs}"$'\n'
        count=$((count + 1))
    done
    actual_bins="$(sort -u <<<"$actual_bins")"

    local drift=0
    # comm 输入经进程替换给出（不接受双 stdin 重定向）；空行由 -z 守卫排除
    while IFS= read -r bin; do
        [[ -z "$bin" ]] && continue
        fail "清单中的示例不存在: examples/src/bin/$bin.rs"
        drift=1
    done < <(comm -23 <(printf '%s' "$doc_bins") <(printf '%s' "$actual_bins"))
    while IFS= read -r bin; do
        [[ -z "$bin" ]] && continue
        fail "示例未列入清单: $bin.rs 存在但 examples/README.md 未记载"
        drift=1
    done < <(comm -13 <(printf '%s' "$doc_bins") <(printf '%s' "$actual_bins"))

    # 文档声明的示例数量必须与实际数量一致；措辞漂移导致零命中时显式提醒
    local declared_found=0
    for doc in examples/README.md README.md README_EN.md; do
        while IFS= read -r declared; do
            [[ -z "$declared" ]] && continue
            declared_found=1
            [[ "$declared" == "$count" ]] || fail "示例数量声明不符: $doc 声明 $declared 个，实际 $count 个"
        done < <(grep -oE '[0-9]+ 个可运行示例|[0-9]+ runnable examples' "$doc" 2>/dev/null \
            | grep -oE '^[0-9]+' | sort -u)
    done
    if [[ "$declared_found" -eq 0 ]]; then
        warn "未找到示例计数声明（N 个可运行示例 / N runnable examples），计数断言空转"
    fi

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

# 首段过滤 SKIP 清单：盲区引用的可见性（不计失败，供人工抽查）
report_skipped_refs() {
    [[ -s "$SKIPPED_REFS" ]] || return 0
    echo ""
    warn "以下引用因首段不是仓库根真实条目而未做存在性核对（防误报取舍，请人工抽查）："
    sort -u "$SKIPPED_REFS" | while IFS= read -r entry; do
        printf '      %s\n' "$entry"
    done
}

# =============================================================================
# 主流程
# =============================================================================

log_section "Limiteron 文档一致性核对"

[[ -f README.md ]] || fail "README.md 不存在，文档一致性核对失去主体对象"

FEATURE_REFS="$(mktemp)"
SKIPPED_REFS="$(mktemp)"
SYMBOLS="$(mktemp)"
trap 'rm -f "$FEATURE_REFS" "$SKIPPED_REFS" "$SYMBOLS"' EXIT

gather_feature_refs "$FEATURE_REFS"
check_paths
check_features "$FEATURE_REFS"
check_api_symbols "$SYMBOLS"
check_examples
check_src_agents
report_skipped_refs

echo ""
if [[ "$VIOLATIONS" -gt 0 ]]; then
    printf '%s文档一致性核对失败：%s 处漂移%s\n' "$RED" "$VIOLATIONS" "$NC"
    exit 1
fi
printf '%s文档一致性核对通过%s\n' "$GREEN" "$NC"
