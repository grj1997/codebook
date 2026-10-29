# Codebook

Windows 本地密码管理器。Tauri 2 + Vue 3 + TypeScript + Rust。

账号、密码、备注、应用关联及浏览器配对记录整体加密，保存在 `%LOCALAPPDATA%\Codebook\vault.cbvault`。没有账号服务、云同步或遥测。

## 开发

需要 Node.js LTS、Rust stable（MSVC）、Microsoft C++ Build Tools 的桌面 C++ 工作负载、Windows SDK 和 WebView2。本项目安装的本地 Rust/MSVC 工具链位于被忽略的 `.tools`，启动脚本会自动加载；也兼容系统安装的工具链。

```powershell
npm.cmd ci
.\scripts\start.ps1
```

普通 `npm run dev` 只能预览界面，不能访问密码库或 Windows 原生功能。

## 使用

1. 创建至少 12 个字符的主密码，再次输入确认。主密码没有找回入口。
2. 添加账户，选择正在运行的应用或 EXE，或者填写 HTTPS 网站地址。程序路径和网站来源用于匹配，显示名称不作为授权依据。
3. 打开目标登录界面，按 `Ctrl+Alt+K`，选择账户后填入，自己点击登录。
4. 也可以先聚焦已关联应用的输入框，在同一位置连续两次右键。原软件单次右键和菜单不被阻止；账户浮窗会额外出现。
5. 如果无法自动识别登录框，先聚焦账号框，在浮窗选择“使用输入顺序”。默认 `账号, Tab, 密码`；支持 `Shift+Tab`、`等待:250`，不支持任意命令。顺序填充要求 UIA 或标准 Win32 Edit 能验证输入框身份；密码步骤还需确认密码框。无法验证的自绘控件会中止。

关闭窗口只是收进托盘。本次进程存续期间持续解锁，包括锁屏、休眠和闲置。可在托盘手动锁定或退出；彻底退出、崩溃、注销及重启后重新输入主密码。

填充前必须恢复到原输入框。原菜单阻挡输入、目标变化或无法恢复焦点时会中止。请关闭菜单并重新调用。管理员权限窗口、安全桌面、第三方自绘控件等可能不支持；不会自动提权。

## Chrome / Edge 扩展

```powershell
npm.cmd run extension:build
.\scripts\register-browser.ps1
```

在 `chrome://extensions` 或 `edge://extensions` 启用开发者模式，“加载已解压的扩展程序”，选择本项目 `browser-extension/dist`。固定扩展 ID 为 `kiakodoohphienfjnedpmbhkplaghnkj`。

运行并解锁 Codebook，在“设置 → 浏览器连接”确认允许此扩展。打开已关联的 HTTPS 登录页，点击扩展或使用全局快捷键调出账户。浏览器多窗口／配置存在歧义时，请从目标页面点击扩展。

安装版会自动注册当前用户的 Native Messaging 主机。开发版通过上述脚本注册。开发版和安装版使用同一个主机名称，后注册者生效；若切换版本，请重新注册正确路径。

首版只支持 HTTPS 顶层常规登录表单。分步登录每一步重新触发，可选择只填账号或密码。暂不处理跨域 iframe、验证码、扫码、Passkey、注册和改密表单。扩展不会自动提交表单，也不持久保存密码。

## 备份与恢复

设置页可导出完整加密备份。首次安装也可直接点击“从加密备份恢复”。恢复时需要备份当时的主密码，验证成功后才替换当前库；原库保留为 `vault.pre-restore.cbvault`。若恢复来源本身是该自动备份，则用带唯一编号的 `vault.pre-restore-….cbvault` 保留原库，避免覆盖恢复来源。保存前一版本位于 `vault.previous.cbvault`。

修改主密码会重新加密当前库，旧备份仍然使用旧密码。复制账号或密码会写入剪贴板，默认 30 秒后仅在内容仍属于本次复制时清理；自动填充不经过剪贴板。

## 检查与打包

```powershell
. .\scripts\dev-env.ps1
npm.cmd run check
cargo test -p codebook-core --features native
cargo build --release -p codebook-native-host
cargo check --workspace
.\scripts\build.ps1
node scripts/smoke-native.mjs
```

NSIS 安装包输出到 `target/release/bundle/nsis`，按当前 Windows 用户安装，并附带 WebView2 离线安装资源。首次构建需要下载依赖和打包工具；应用日常使用无需 Codebook 服务端。

密码学使用 RustCrypto Argon2id（64 MiB、3 轮、并行度 1）及 AES-256-GCM。主密码与解锁密钥不持久化。前端仅持有编辑或显式查看时必要的明文，不使用 localStorage、密码事件广播或明文日志。解锁后明文及密钥存在进程内存中，不能防护已经控制当前 Windows 用户会话的恶意程序。

原生交互需在真实 Windows 桌面验收；单元测试不能代替具体第三方软件的兼容性验证。测试产生的临时目录会保留，不进行批量清理；卸载时保留密码库与备份。

交互式 UIA 测试使用独立的虚构登录窗口，可在桌面终端运行：

```powershell
cargo test -p codebook-core --features native --test windows_live uia_fill -- --ignored --nocapture
```

当前自动执行环境未能把该测试窗口切到前台，因此完整 UIA 填充验收尚未通过；窗口标题变化的原生回归测试已通过。Chrome／Edge 实际加载扩展、右键菜单与浮窗共存、安装和卸载也仍需人工交互验收。

`smoke-native.mjs` 只启动本项目的临时实例，将 LOCALAPPDATA 指向项目内隔离目录，实测启动、单实例及本机通信，不注册主机或访问已有密码库。结束时仅终止自己启动的进程；测试数据目录保留。
