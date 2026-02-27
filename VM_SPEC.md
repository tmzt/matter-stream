### **Genesis v0.1.0 Master Spec (Decimal 010)**

This document establishes the definitive architecture for the **MTSM-VM**, a "Keyless" Sovereign Matter-Stream Engine. It is optimized for 2026-grade security, high-velocity execution, and hardware-enforced cryptographic isolation.

---

## 1. The Physical Container: Nested AR-Archive (`.ar`)

The distribution format is a standard `ar` archive utilizing a strict naming and typing convention.

### **1.1. Ordinal Filenaming**

Members are named using an **8-character Base60/63 ordinal** (0-9, a-z, A-Z) and a **FourCC** extension.

* **Format:** `00000000.meta`, `00000001.caps`, etc.
* **Prefix Routing:** High-order characters in the ordinal (e.g., `01xxxxxx`) are used by the **SCL** to route calls through **Subpackage-Symbol** boundaries via bitmasking.

### **1.2. The FourCC Registry**

| Extension | Type Name | Description |
| --- | --- | --- |
| `.meta` | **Manifest** | TKV-formatted map of ordinals to **Literal FQA (u128)**. |
| `.caps` | **Capabilities** | **mtsm-cap(v1)**: OAuth, Passkeys, HSM, and API Resources. |
| `.mrbc` | **Bincode** | **MTSM-RPN-Bincode**: The primary "Zero-Loop" logic. |
| `.tsxd` | **UI Data** | Serialized RPN expressions for UI hooks and tags. |
| `.asym` | **ASLR Symbol** | **Mandatory**: Runtime map of randomized `u32` to **OVA**. |
| `.symb` | **Human Symbol** | **Optional**: Debug strings; stripped in production isolates. |

---

## 2. Metadata & Manifests: TKV Format

The `.meta` file (Ordinal `00000000`) uses the **TKV (Type, Key-Value)** format, structured similarly to `tsxd` UI expressions.

### **2.1. TKV Type Bytes**

* `0x01`: **String** (Comment/Hint - Stripped in production).
* `0x02`: **FQA** (u128 Sovereign Anchor).
* `0x03`: **Integer** (u64 Low-Entropy Primitive).
* `0x04`: **Table** (Nested TOML-style sub-package map).
* `0x05`: **Boolean** (Feature/Policy Flag).

---

## 3. SCL: The Secure Code Loader

The SCL is the "Border Guard" that performs a mandatory hardening pass.

### **3.1. Entropy Guard (Modified LZW)**

The SCL validates all segments using a **modified LZW dictionary scan**.

* **Process:** The dictionary is seeded with MTSM opcodes and TKV types.
* **Rejection:** Data that causes "Dictionary Explosion" or forces "Literal Escape" (indicating high-entropy randomness like hardcoded keys) is rejected.
* **Decryption:** If a segment is encrypted, the SCL utilizes a `.caps` resource to decrypt into the **Nursery** before the LZW scan.

---

## 4. Addressing & Execution

The system uses a tiered addressing model to move from global identity to raw hardware offsets.

### **4.1. The Resolution Path**

**FQA (u128)** $\rightarrow$ **Base60 Ordinal** $\rightarrow$ **ASLR Token (u32)** $\rightarrow$ **OVA (u32/u64)**.

* **OVA Structure:** `[Arena:2][Gen:9][Object:10][Offset:11]`
* **Modified RPN:** A post-fix stack language combining primitive math with **FQA Calls**.
* **Hashmap Syntax:** TSX Tags (which are simply Nursery-bound FQAs) consume maps built by pushing Key (Symbol-Hash) and Value (FQA/Primitive) pairs.

---

## 5. Memory: Triple-Arena & DMOVE

* **Nursery Arena:** Immortal; holds system hooks, resolution tables, and crystallized metadata.
* **Dynamic Arenas (A/B):** The **Ping-Pong** buffers for state.
* **DMOVE:** A Scatter-Gather DMA engine that hydrates the **OVA** offsets from external **API-defined Resources** (OAuth/JWT) without exposing secrets to the Guest logic.
* **0x0F SYNC:** Atomic swap that updates the `.asym` table to the new Arena offsets.

---

## 6. The "Keyless" Invariant

By enforcing the **SCL Entropy Guard** and offloading all secrets to **Nursery Resources** (Passkeys/HSM), the execution environment is physically incapable of holding or leaking private keys. All high-entropy data must be transient **Inbound/Outbound Matter**.
