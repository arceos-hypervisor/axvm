<h1 align="center">axvm</h1>

<p align="center">面向 ArceOS Hypervisor 的虚拟机资源管理</p>

<div align="center">

[![Crates.io](https://img.shields.io/crates/v/axvm.svg)](https://crates.io/crates/axvm)
[![Docs.rs](https://docs.rs/axvm/badge.svg)](https://docs.rs/axvm)
[![Rust](https://img.shields.io/badge/edition-2024-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/arceos-hypervisor/axvm/blob/main/LICENSE)

</div>

[English](README.md) | 中文

# Introduction

`axvm` 是一个面向 ArceOS hypervisor 的 `#![no_std]` 虚拟机资源管理 crate。它建立在 `axaddrspace`、`axvcpu`、`axdevice` 和 `axvmconfig` 之上，用于统一协调 guest 地址空间构建、虚拟 CPU 初始化、模拟设备管理以及虚拟机生命周期控制。

该库导出五个核心公开类型：

- **`AxVM`** - 主要的虚拟机对象
- **`AxVMRef`** - `AxVM` 的共享引用类型
- **`AxVCpuRef`** - VM 中 vCPU 的共享引用类型
- **`VMMemoryRegion`** - 描述 guest 内存映射区域
- **`VMStatus`** - 表示 loading、running、stopped 等虚拟机生命周期状态

此外还提供：

- **`config`** - 包含 `AxVMConfig`、`AxVMCrateConfig` 等 VM 配置类型
- **`AxVMPerCpu`** - 与架构无关的每 CPU 虚拟化辅助类型
- **`has_hardware_support()`** - 检查当前硬件是否支持虚拟化

## Quick Start

### Requirements

- Rust nightly 工具链
- Rust 组件：rust-src、clippy、rustfmt

```bash
# 安装 rustup（如果尚未安装）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装 nightly 工具链与所需组件
rustup install nightly
rustup component add rust-src clippy rustfmt --toolchain nightly
```

### Run Check and Test

```bash
# 1. 进入仓库目录
cd axvm

# 2. 代码检查
./scripts/check.sh

# 3. 运行测试
./scripts/test.sh
```

## Integration

### Installation

将以下依赖加入 `Cargo.toml`：

```toml
[dependencies]
axvm = "0.3.0"
```

### Example

```rust
use axvm::{
    config::{AxVMConfig, AxVMCrateConfig},
    AxVM, VMStatus,
};

fn main() {
    let crate_config = AxVMCrateConfig::default();
    let config = AxVMConfig::from(crate_config);

    let vm = AxVM::new(config).unwrap();
    assert_eq!(vm.vm_status(), VMStatus::Loading);

    // 初始化 guest 资源，例如地址空间、vCPU 和设备。
    vm.init().unwrap();

    // 初始化完成后启动虚拟机。
    vm.boot().unwrap();
    assert_eq!(vm.vm_status(), VMStatus::Running);

    // 查询由 VM 对象管理的资源。
    let _vcpu_num = vm.vcpu_num();
    let _ept_root = vm.ept_root();
    let _devices = vm.get_devices();
}
```

### Documentation

生成并查看 API 文档：

```bash
cargo doc --no-deps --open
```

在线文档： [docs.rs/axvm](https://docs.rs/axvm)

# Contributing

1. Fork 仓库并创建分支
2. 本地运行检查：`./scripts/check.sh`
3. 本地运行测试：`./scripts/test.sh`
4. 提交 PR 并通过 CI 检查

# License

本项目基于 Apache License 2.0 许可证发布。详见 [LICENSE](LICENSE)。
