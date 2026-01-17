//! 规则匹配器示例
//!
//! 本示例演示规则匹配器的使用方式。
//!
//! 运行方式: `cargo run --example matcher`

#[tokio::main]
async fn main() {
    println!("=== 规则匹配器示例 ===\n");

    println!("--- 规则匹配器功能 ---\n");
    println!("RuleMatcher 提供:");
    println!("  - add_rule(): 添加规则");
    println!("  - match_all(): 匹配所有规则");
    println!("  - match_rate_limit(): 获取匹配的速率限制");

    println!("\n--- 条件类型 ---\n");
    println!("支持的匹配条件:");
    println!("  - IpInRange: IP 在指定范围内");
    println!("  - IpNotInRange: IP 不在指定范围内");
    println!("  - PathPrefix: 路径前缀匹配");
    println!("  - PathExact: 路径精确匹配");
    println!("  - UserIdEquals: 用户 ID 等于");
    println!("  - HeaderEquals: 请求头等于");

    println!("\n--- 逻辑运算符 ---\n");
    println!("  - And: 所有条件都必须满足");
    println!("  - Or: 任意条件满足即可");
    println!("  - Not: 条件取反");

    println!("\n--- 使用方式 ---\n");
    println!("  let mut matcher = RuleMatcher::new(rules);");
    println!("  let matched = matcher.match_all(&context);");

    println!("\n=== 示例完成 ===");
}
