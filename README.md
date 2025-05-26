# ⚡ LND HTLC UI

<div align="center">

![Lightning Network](ui/views/images/lightning-logo.png)

A modern, user-friendly interface for managing Hash Time-Locked Contracts (HTLCs) on the Lightning Network.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Lightning](https://img.shields.io/badge/Lightning-Network-orange.svg)](https://lightning.network)

</div>

## 🚀 Features

- 🔒 Secure HTLC management interface
- ⚡ Real-time Lightning Network interactions
- 🎨 Modern and intuitive UI built with Slint
- 💼 Support for both LND and LIT daemon
- 🔄 Automatic configuration management
- 📊 Transaction history tracking
- 🔐 Secure key management
- 📱 Cross-platform support

## 🛠️ Prerequisites

- Rust (latest stable version)
- LND or Lightning Terminal (LiT) daemon
- macOS 10.13 or later (for macOS users)

## 📦 Installation

1. Clone the repository:
```bash
git clone https://github.com/yourusername/lnd-htlc-ui.git
cd lnd-htlc-ui
```

2. Build the project:
```bash
cargo build --release
```

3. Run the application:
```bash
cargo run --release
```

## 📱 Creating Application Bundles

The project supports `cargo-bundle` for creating native application packages. To create a bundle:

1. Install cargo-bundle:
```bash
cargo install cargo-bundle
```

2. Create a bundle for your platform:
```bash
cargo bundle --release
```

This will create:
- On macOS: A `.app` bundle in `target/release/bundle/osx`
- On Windows: An installer in `target/release/bundle/windows`
- On Linux: Appropriate packages for your distribution in `target/release/bundle/linux`

The bundle includes:
- Application icon
- Required resources
- System integration files
- Minimum OS version requirements (macOS 10.13+)

## ⚙️ Configuration

The application looks for configuration in the following locations:
- Custom config file specified via command line
- `~/.lnd/lnd.conf` (for LND)
- `~/.lit/lit.conf` (for Lightning Terminal)

Example configuration:
```toml
[Application]
network=testnet
macaroon_path=~/.lnd/data/chain/bitcoin/testnet/admin.macaroon
tls_cert_path=~/.lnd/tls.cert
```

## 🔧 Development

To start development:

1. Install dependencies:
```bash
cargo check
```

2. Run in development mode:
```bash
cargo run
```

3. Build for production:
```bash
cargo build --release
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Lightning Network Daemon (LND) team
- Lightning Terminal (LiT) team
- Rust community
- Slint UI framework team

---

<div align="center">
Made with ❤️
</div>
