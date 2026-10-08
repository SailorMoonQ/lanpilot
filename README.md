# LanPilot

用手机在局域网内遥控电脑：触摸板、媒体控制、文本发送、自定义快捷指令面板。

- 手机端：iOS / Android（Flutter）
- 电脑端：Windows / Linux（Rust）

详见 [设计文档](docs/superpowers/specs/2026-10-07-lanpilot-design.md)。

## 开发状态

- `core`：共享协议、加密传输、配对、发现（已完成）
- `agent`：电脑端无界面版本（M1）：`cargo run -p lanpilot-agent -- run --pair`；加 `--mock-input` 只记录输入而不注入（任何系统，含 macOS，可配合 `RUST_LOG=lanpilot_input=debug` 查看）
- `lpctl`：开发用命令行客户端：`cargo run -p lpctl -- --help`

Linux 需要先安装 uinput 权限规则：见 `packaging/linux/70-lanpilot-uinput.rules`。手动测试清单：`docs/e2e/m1-checklist.md`。

## 许可证

以 [MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE) 许可证双重授权，任选其一。
