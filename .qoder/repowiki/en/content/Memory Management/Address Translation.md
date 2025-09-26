
# Address Translation

<cite>
**Referenced Files in This Document**   
- [vm.rs](file://src/vm.rs)
- [hal.rs](file://src/hal.rs)
</cite>

## Table of Contents
1. [Introduction](#introduction)
2. [Two-Stage Address Translation Mechanism](#two-stage-address-translation-mechanism)
3. [Guest Virtual to Guest Physical Address Translation](#guest-virtual-to-guest-physical-address-translation)
4. [Guest Physical to Host Physical Address Translation via EPT](#guest-physical-to-host-physical-address-translation-via-ept)
5. [Role of ept_root() Method](#role-of-ept_root-method)
6. [Integration with HAL Paging Handler](#integration-with-hal-paging-handler)
7. [VM Initialization and Memory Mapping](#vm-initialization-and-memory-mapping)
8. [Data Flow During Address Translation](#data-flow-during-address-translation)
9. [Performance Implications](#performance-implications)
10. [Security Aspects](#security-aspects)

## Introduction
The axvm hypervisor implements a two-stage address translation mechanism to securely and efficiently manage memory access for virtual machines (VMs). This system enables guest virtual addresses (GVAs) to be translated into host physical addresses (HPAs) through an intermediate step involving guest physical addresses (GPAs), leveraging hardware-assisted virtualization features such as Extended Page Tables (E