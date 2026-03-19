// 测试兼容模块
// 导出 DecisionNode::new 和 DecisionChain::new 方法到测试命名空间

pub use crate::decision_chain::DecisionChain;
pub use crate::decision_chain::DecisionChainBuilder;
pub use crate::decision_chain::DecisionNode;
pub use crate::decision_chain::DecisionNodeBuilder;
