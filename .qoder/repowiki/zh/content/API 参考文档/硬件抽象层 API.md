
# 硬件抽象层 API

<cite>
**本文档引用的文件**   
- [hal.rs](file://src/hal.rs)
- [vm.rs](file://src/vm.rs)
- [vcpu.rs](file://src/vcpu.rs)
- [lib.rs](file://src/lib.rs)
</cite>

## 目录
1. [引言](#引言)
2. [核心方法规范](#核心方法规范)
3. [关联类型说明](#关联类型说明)
4. [调用上下文与线程安全](#调用上下文与线程安全)
5. [错误处理机制](#错误处理机制)
6. [架构差异实现](#架构差异实现)

## 引言
`AxVMHal` trait 定义了虚拟机监控器（VMM）与底层宿主系统（内核或超级管理程序）之间的契约接口。该接口作为硬件抽象层（HAL），为 VMM 提供对物理资源的访问能力，包括内存管理、地址转换、时间获取和中断注入等功能。此文档旨在提供 `AxVMHal` 接口的权威性规范，详细描述每个方法的语义、行为、调用要求及跨平台实现差异。

**Section sources**
- [hal.rs](file://src/hal.rs#L0-L43)

## 核心方法规范

### alloc_memory_region_at() 与 dealloc_memory_region_at()
这两个方法负责在指定的物理地址处预留和释放内存区域。

`alloc_memory_region_at(base: HostPhysAddr, size: usize) -> bool` 方法尝试在给定的物理基地址 `base` 处分配大小为 `size` 的连续内存区域。如果分配成功，返回 `true`；否则返回 `false`。此操作通常用于确保特定物理地址范围被当前 VM 专用，防止被其他实体使用。失败可能由地址已被占用、权限不足或物理内存不足等原因导致。

`dealloc_memory_region_at(base: HostPhysAddr, size: usize)`