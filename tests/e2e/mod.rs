//! 端到端测试模块
//!
//! 测试完整的业务流程和场景

#[allow(unused_imports)]
mod multi_rule_cascade;
#[cfg(feature = "quota-control")]
#[allow(unused_imports)]
mod quota_overdraft;
#[cfg(feature = "ban-manager")]
#[allow(unused_imports)]
mod rate_limit_to_ban;

#[cfg(feature = "quota-control")]
#[allow(unused_imports)]
pub use quota_overdraft::*;
#[cfg(feature = "ban-manager")]
#[allow(unused_imports)]
pub use rate_limit_to_ban::*;
