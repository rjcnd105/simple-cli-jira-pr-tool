# σ₃: Technical Context

*v1.0 | Created: 2025-12-30 | Updated: 2025-12-30*
*Π: Π₂ | Ω: Ω₁*

## 🛠️ Technology Stack

- **Language**: Rust (edition 2024)
- **Async Runtime**: `tokio` (v1.x, full features)
- **HTTP Client**: `reqwest` (v0.12, json, rustls-tls)
- **CLI**: `clap` (v4, derive)
- **Serialization**: `serde`, `serde_json`
- **Error Handling**: `anyhow`
- **Environment**: `dotenvy`
- **Regex**: `regex` (v1.12.2)

## ⚙️ Development Environment

- **Package Manager**: Cargo
- **Tool Manager**: `mise` (mise.toml 사용)
- **API Dependencies**:
  - Bitbucket Cloud REST API v2.0
  - Jira Cloud REST API v3
