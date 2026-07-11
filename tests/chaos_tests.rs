// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! 混沌测试入口
//!
//! 故障注入、延迟注入等混沌测试

mod chaos;

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::chaos::*;
}
