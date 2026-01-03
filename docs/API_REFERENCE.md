<div align="center">

# 📘 API Reference

### Complete API Documentation

[🏠 Home](../README.md) • [📖 User Guide](USER_GUIDE.md) • [🏗️ Architecture](ARCHITECTURE.md)

---

</div>

## 📋 Table of Contents

- [Overview](#overview)
- [Core API](#core-api)
  - [Initialization](#initialization)
  - [Configuration](#configuration)
  - [Cipher Operations](#cipher-operations)
  - [Key Management](#key-management)
- [Algorithms](#algorithms)
- [Error Handling](#error-handling)
- [Type Definitions](#type-definitions)
- [Examples](#examples)

---

## Overview

<div align="center">

### 🎯 API Design Principles

</div>

<table>
<tr>
<td width="25%" align="center">
<img src="https://img.icons8.com/fluency/96/000000/easy.png" width="64"><br>
<b>Simple</b><br>
Intuitive and easy to use
</td>
<td width="25%" align="center">
<img src="https://img.icons8.com/fluency/96/000000/security-checked.png" width="64"><br>
<b>Safe</b><br>
Type-safe and secure by default
</td>
<td width="25%" align="center">
<img src="https://img.icons8.com/fluency/96/000000/module.png" width="64"><br>
<b>Composable</b><br>
Build complex workflows easily
</td>
<td width="25%" align="center">
<img src="https://img.icons8.com/fluency/96/000000/documentation.png" width="64"><br>
<b>Well-documented</b><br>
Comprehensive documentation
</td>
</tr>
</table>

---

## Core API

### Initialization

<div align="center">

#### 🚀 Getting Started

</div>

---

#### `init()`

Initialize the library with default configuration.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn init() -> Result<(), Error>
```

</td>
</tr>
<tr>
<td><b>Description</b></td>
<td>Initializes the library with default settings. Must be called before using any other API.</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;(), Error&gt;</code> - Ok on success, Error on failure</td>
</tr>
<tr>
<td><b>Errors</b></td>
<td>

- `Error::AlreadyInitialized` - Library already initialized
- `Error::InitializationFailed` - Initialization failed

</td>
</tr>
</table>

**Example:**

```rust
use project_name::init;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the library
    init()?;
    
    println!("✅ Library initialized successfully");
    Ok(())
}
```

---

#### `init_with_config()`

Initialize the library with custom configuration.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn init_with_config(config: Config) -> Result<(), Error>
```

</td>
</tr>
<tr>
<td><b>Parameters</b></td>
<td>

- `config: Config` - Configuration options

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;(), Error&gt;</code></td>
</tr>
</table>

**Example:**

```rust
use project_name::{init_with_config, Config};

let config = Config::builder()
    .thread_pool_size(8)
    .cache_size(2048)
    .build()?;

init_with_config(config)?;
```

---

### Configuration

<div align="center">

#### ⚙️ Configuration Builder

</div>

---

#### `Config`

Configuration struct for customizing library behavior.

<table>
<tr>
<td width="30%"><b>Type</b></td>
<td width="70%">

```rust
pub struct Config {
    pub thread_pool_size: usize,
    pub cache_size: usize,
    pub log_level: LogLevel,
    pub enable_metrics: bool,
    // ... more fields
}
```

</td>
</tr>
</table>

---

#### `Config::builder()`

Create a new configuration builder.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn builder() -> ConfigBuilder
```

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>ConfigBuilder</code> - Configuration builder instance</td>
</tr>
</table>

**Builder Methods:**

<details>
<summary><b>View All Methods</b></summary>

| Method | Type | Default | Description |
|--------|------|---------|-------------|
| `thread_pool_size(usize)` | usize | 4 | Number of worker threads |
| `cache_size(usize)` | usize | 1024 | Cache size in MB |
| `log_level(LogLevel)` | LogLevel | Info | Logging verbosity |
| `enable_metrics(bool)` | bool | false | Enable metrics collection |
| `enable_audit(bool)` | bool | true | Enable audit logging |
| `build()` | - | - | Build the configuration |

</details>

**Example:**

```rust
use project_name::{Config, LogLevel};

let config = Config::builder()
    .thread_pool_size(8)
    .cache_size(2048)
    .log_level(LogLevel::Debug)
    .enable_metrics(true)
    .build()?;
```

---

### Cipher Operations

<div align="center">

#### 🔐 Encryption and Decryption

</div>

---

#### `Cipher`

Main cipher struct for encryption/decryption operations.

<table>
<tr>
<td width="30%"><b>Type</b></td>
<td width="70%">

```rust
pub struct Cipher {
    algorithm: Algorithm,
    // internal fields
}
```

</td>
</tr>
</table>

---

#### `Cipher::new()`

Create a new cipher instance.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn new(algorithm: Algorithm) -> Result<Self, Error>
```

</td>
</tr>
<tr>
<td><b>Parameters</b></td>
<td>

- `algorithm: Algorithm` - Cryptographic algorithm to use

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;Cipher, Error&gt;</code></td>
</tr>
<tr>
<td><b>Errors</b></td>
<td>

- `Error::AlgorithmNotSupported` - Algorithm not available
- `Error::InitializationFailed` - Failed to initialize cipher

</td>
</tr>
</table>

**Example:**

```rust
use project_name::{Cipher, Algorithm};

let cipher = Cipher::new(Algorithm::AES256GCM)?;
```

---

#### `Cipher::encrypt()`

Encrypt data using the specified key.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn encrypt(
    &self,
    key_manager: &KeyManager,
    key_id: &str,
    plaintext: &[u8]
) -> Result<Vec<u8>, Error>
```

</td>
</tr>
<tr>
<td><b>Parameters</b></td>
<td>

- `key_manager: &KeyManager` - Key manager instance
- `key_id: &str` - ID of the encryption key
- `plaintext: &[u8]` - Data to encrypt

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;Vec&lt;u8&gt;, Error&gt;</code> - Encrypted ciphertext</td>
</tr>
<tr>
<td><b>Errors</b></td>
<td>

- `Error::KeyNotFound` - Key ID not found
- `Error::InvalidKeyState` - Key not in active state
- `Error::EncryptionFailed` - Encryption operation failed

</td>
</tr>
</table>

**Example:**

```rust
use project_name::{Cipher, KeyManager, Algorithm};

let km = KeyManager::new()?;
let key_id = km.generate_key(Algorithm::AES256GCM)?;
let cipher = Cipher::new(Algorithm::AES256GCM)?;

let plaintext = b"Secret message";
let ciphertext = cipher.encrypt(&km, &key_id, plaintext)?;
```

<details>
<summary><b>📝 Notes</b></summary>

- The returned ciphertext includes authentication tag
- A random nonce/IV is generated for each encryption
- The same plaintext will produce different ciphertexts (IND-CPA security)

</details>

---

#### `Cipher::decrypt()`

Decrypt data using the specified key.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn decrypt(
    &self,
    key_manager: &KeyManager,
    key_id: &str,
    ciphertext: &[u8]
) -> Result<Vec<u8>, Error>
```

</td>
</tr>
<tr>
<td><b>Parameters</b></td>
<td>

- `key_manager: &KeyManager` - Key manager instance
- `key_id: &str` - ID of the decryption key
- `ciphertext: &[u8]` - Data to decrypt

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;Vec&lt;u8&gt;, Error&gt;</code> - Decrypted plaintext</td>
</tr>
<tr>
<td><b>Errors</b></td>
<td>

- `Error::KeyNotFound` - Key ID not found
- `Error::DecryptionFailed` - Decryption or authentication failed
- `Error::InvalidCiphertext` - Malformed ciphertext

</td>
</tr>
</table>

**Example:**

```rust
let plaintext = cipher.decrypt(&km, &key_id, &ciphertext)?;
assert_eq!(plaintext, b"Secret message");
```

---

#### `Cipher::sign()`

Create a digital signature.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn sign(
    &self,
    key_manager: &KeyManager,
    key_id: &str,
    message: &[u8]
) -> Result<Vec<u8>, Error>
```

</td>
</tr>
<tr>
<td><b>Parameters</b></td>
<td>

- `key_manager: &KeyManager` - Key manager instance
- `key_id: &str` - ID of the signing key
- `message: &[u8]` - Data to sign

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;Vec&lt;u8&gt;, Error&gt;</code> - Digital signature</td>
</tr>
<tr>
<td><b>Applicable Algorithms</b></td>
<td>ECDSA, RSA, Ed25519, SM2</td>
</tr>
</table>

**Example:**

```rust
use project_name::{Cipher, KeyManager, Algorithm};

let km = KeyManager::new()?;
let key_id = km.generate_key(Algorithm::ECDSAP256)?;
let signer = Cipher::new(Algorithm::ECDSAP256)?;

let message = b"Important message";
let signature = signer.sign(&km, &key_id, message)?;
```

---

#### `Cipher::verify()`

Verify a digital signature.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn verify(
    &self,
    key_manager: &KeyManager,
    key_id: &str,
    message: &[u8],
    signature: &[u8]
) -> Result<bool, Error>
```

</td>
</tr>
<tr>
<td><b>Parameters</b></td>
<td>

- `key_manager: &KeyManager` - Key manager instance
- `key_id: &str` - ID of the verification key
- `message: &[u8]` - Original message
- `signature: &[u8]` - Signature to verify

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;bool, Error&gt;</code> - true if valid, false otherwise</td>
</tr>
</table>

**Example:**

```rust
let is_valid = signer.verify(&km, &key_id, message, &signature)?;
assert!(is_valid);
```

---

### Key Management

<div align="center">

#### 🔑 Key Lifecycle Operations

</div>

---

#### `KeyManager`

Manages cryptographic keys throughout their lifecycle.

<table>
<tr>
<td width="30%"><b>Type</b></td>
<td width="70%">

```rust
pub struct KeyManager {
    // internal fields
}
```

</td>
</tr>
</table>

---

#### `KeyManager::new()`

Create a new key manager instance.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn new() -> Result<Self, Error>
```

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;KeyManager, Error&gt;</code></td>
</tr>
</table>

**Example:**

```rust
use project_name::KeyManager;

let km = KeyManager::new()?;
```

---

#### `KeyManager::generate_key()`

Generate a new cryptographic key.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn generate_key(&self, algorithm: Algorithm) -> Result<String, Error>
```

</td>
</tr>
<tr>
<td><b>Parameters</b></td>
<td>

- `algorithm: Algorithm` - Algorithm for the key

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;String, Error&gt;</code> - Unique key ID</td>
</tr>
<tr>
<td><b>Errors</b></td>
<td>

- `Error::AlgorithmNotSupported` - Algorithm not available
- `Error::KeyGenerationFailed` - Failed to generate key

</td>
</tr>
</table>

**Example:**

```rust
use project_name::{KeyManager, Algorithm};

let km = KeyManager::new()?;
let key_id = km.generate_key(Algorithm::AES256GCM)?;
println!("Generated key: {}", key_id);
```

---

#### `KeyManager::generate_key_with_alias()`

Generate a key with a human-readable alias.

<table>
<tr>
<td width="30%"><b>Signature</b></td>
<td width="70%">

```rust
pub fn generate_key_with_alias(
    &self,
    algorithm: Algorithm,
    alias: &str
) -> Result<String, Error>
```

</td>
</tr>
<tr>
<td><b>Parameters</b></td>
<td>

- `algorithm: Algorithm` - Algorithm for the key
- `alias: &str` - Human-readable name

</td>
</tr>
<tr>
<td><b>Returns</b></td>
<td><code>Result&lt;String, Error&gt;</code> - Key ID</td>
</tr>
</table>

**Example:**

```rust
let key_id = km.generate_key_with_alias(
    Algorithm::AES256GCM,
    "database-encryption-key"
)?;
```

---

## Algorithms

<div align="center">

#### 🔐 Supported Cryptographic Algorithms

</div>

### `Algorithm` Enum

<table>
<tr>
<td width="30%"><b>Definition</b></td>
<td width="70%">

```rust
pub enum Algorithm {
    // Symmetric Encryption
    AES128GCM,
    AES192GCM,
    AES256GCM,
    SM4GCM,
    
    // Asymmetric Signatures
    ECDSAP256,
    ECDSAP384,
    ECDSAP521,
    RSA2048,
    RSA3072,
    RSA4096,
    Ed25519,
    SM2,
}
```

</td>
</tr>
</table>

### Algorithm Details

<details open>
<summary><b>🔐 Symmetric Encryption</b></summary>

<table>
<tr>
<th>Algorithm</th>
<th>Key Size</th>
<th>Security Level</th>
<th>Performance</th>
<th>Use Case</th>
</tr>
<tr>
<td><b>AES-128-GCM</b></td>
<td>128-bit</td>
<td>🟢 High</td>
<td>⚡⚡⚡ Very Fast</td>
<td>General purpose</td>
</tr>
<tr>
<td><b>AES-192-GCM</b></td>
<td>192-bit</td>
<td>🟢 High</td>
<td>⚡⚡ Fast</td>
<td>Extra security</td>
</tr>
<tr>
<td><b>AES-256-GCM</b></td>
<td>256-bit</td>
<td>🟢 Very High</td>
<td>⚡⚡ Fast</td>
<td>Maximum security</td>
</tr>
<tr>
<td><b>SM4-GCM</b></td>
<td>128-bit</td>
<td>🟢 High</td>
<td>⚡ Moderate</td>
<td>Chinese standards</td>
</tr>
</table>

</details>

<details>
<summary><b>✍️ Digital Signatures</b></summary>

<table>
<tr>
<th>Algorithm</th>
<th>Key Size</th>
<th>Security Level</th>
<th>Signature Size</th>
<th>Use Case</th>
</tr>
<tr>
<td><b>ECDSA-P256</b></td>
<td>256-bit</td>
<td>🟢 High</td>
<td>~64 bytes</td>
<td>Modern standard</td>
</tr>
<tr>
<td><b>ECDSA-P384</b></td>
<td>384-bit</td>
<td>🟢 Very High</td>
<td>~96 bytes</td>
<td>High security</td>
</tr>
<tr>
<td><b>RSA-2048</b></td>
<td>2048-bit</td>
<td>🟢 High</td>
<td>256 bytes</td>
<td>Legacy support</td>
</tr>
<tr>
<td><b>Ed25519</b></td>
<td>256-bit</td>
<td>🟢 High</td>
<td>64 bytes</td>
<td>Fast verification</td>
</tr>
<tr>
<td><b>SM2</b></td>
<td>256-bit</td>
<td>🟢 High</td>
<td>~64 bytes</td>
<td>Chinese standards</td>
</tr>
</table>

</details>

---

## Error Handling

<div align="center">

#### 🚨 Error Types and Handling

</div>

### `Error` Enum

```rust
pub enum Error {
    // Initialization Errors
    AlreadyInitialized,
    NotInitialized,
    InitializationFailed,
    
    // Key Errors
    KeyNotFound,
    KeyGenerationFailed,
    InvalidKeyState,
    
    // Cryptographic Errors
    EncryptionFailed,
    DecryptionFailed,
    SignatureFailed,
    VerificationFailed,
    
    // Algorithm Errors
    AlgorithmNotSupported,
    AlgorithmNotFound,
    
    // I/O Errors
    IoError(std::io::Error),
    
    // Custom errors
    Custom(String),
}
```

### Error Handling Pattern

<table>
<tr>
<td width="50%">

**Pattern Matching**
```rust
match operation() {
    Ok(result) => {
        println!("Success: {:?}", result);
    }
    Err(Error::KeyNotFound) => {
        eprintln!("Key not found");
    }
    Err(Error::EncryptionFailed) => {
        eprintln!("Encryption failed");
    }
    Err(e) => {
        eprintln!("Error: {:?}", e);
    }
}
```

</td>
<td width="50%">

**? Operator**
```rust
fn process_data() -> Result<(), Error> {
    init()?;
    
    let km = KeyManager::new()?;
    let key = km.generate_key(
        Algorithm::AES256GCM
    )?;
    
    let cipher = Cipher::new(
        Algorithm::AES256GCM
    )?;
    
    Ok(())
}
```

</td>
</tr>
</table>

---

## Type Definitions

### Common Types

<table>
<tr>
<td width="50%">

**Key ID**
```rust
pub type KeyId = String;
```

**Algorithm Type**
```rust
pub enum Algorithm { /* ... */ }
```

</td>
<td width="50%">

**Result Type**
```rust
pub type Result<T> = 
    std::result::Result<T, Error>;
```

**Log Level**
```rust
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}
```

</td>
</tr>
</table>

---

## Examples

<div align="center">

### 💡 Common Usage Patterns

</div>

### Example 1: Basic Encryption

```rust
use project_name::{init, Cipher, KeyManager, Algorithm};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize
    init()?;
    
    // Setup
    let km = KeyManager::new()?;
    let key_id = km.generate_key(Algorithm::AES256GCM)?;
    let cipher = Cipher::new(Algorithm::AES256GCM)?;
    
    // Encrypt
    let plaintext = b"Hello, World!";
    let ciphertext = cipher.encrypt(&km, &key_id, plaintext)?;
    
    // Decrypt
    let decrypted = cipher.decrypt(&km, &key_id, &ciphertext)?;
    
    assert_eq!(plaintext, &decrypted[..]);
    println!("✅ Success!");
    
    Ok(())
}
```

### Example 2: Digital Signatures

```rust
use project_name::{init, Cipher, KeyManager, Algorithm};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init()?;
    
    let km = KeyManager::new()?;
    let key_id = km.generate_key(Algorithm::ECDSAP256)?;
    let signer = Cipher::new(Algorithm::ECDSAP256)?;
    
    // Sign
    let message = b"Important document";
    let signature = signer.sign(&km, &key_id, message)?;
    
    // Verify
    let is_valid = signer.verify(&km, &key_id, message, &signature)?;
    assert!(is_valid);
    
    println!("✅ Signature verified!");
    
    Ok(())
}
```

### Example 3: Advanced Configuration

```rust
use project_name::{init_with_config, Config, LogLevel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::builder()
        .thread_pool_size(8)
        .cache_size(2048)
        .log_level(LogLevel::Debug)
        .enable_metrics(true)
        .enable_audit(true)
        .build()?;
    
    init_with_config(config)?;
    
    // Use the library...
    
    Ok(())
}
```

---

<div align="center">

**[📖 User Guide](USER_GUIDE.md)** • **[🏗️ Architecture](ARCHITECTURE.md)** • **[🏠 Home](../README.md)**

Made with ❤️ by the Documentation Team

[⬆ Back to Top](#-api-reference)

</div>