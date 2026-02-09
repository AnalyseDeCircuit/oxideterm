# OxideTerm 开发指南

> 本文档介绍如何搭建开发环境、项目结构和贡献流程。

## 目录

1. [环境准备](#环境准备)
2. [项目结构](#项目结构)
3. [开发工作流](#开发工作流)
4. [代码规范](#代码规范)
5. [测试](#测试)
6. [调试技巧](#调试技巧)
7. [发布流程](#发布流程)
8. [贡献指南](#贡献指南)

---

## 环境准备

### 系统要求

| 平台 | 要求 |
|------|------|
| **macOS** | 10.15+ (Catalina), Xcode Command Line Tools |
| **Windows** | 10/11, Visual Studio Build Tools 2019+ |
| **Linux** | Ubuntu 20.04+ / Fedora 33+ |

### 安装依赖

#### 1. Rust 工具链

```bash
# 安装 rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装稳定版工具链
rustup default stable

# 验证安装
rustc --version  # 需要 1.75+
```

#### 2. Node.js

```bash
# 使用 nvm (推荐)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 18
nvm use 18

# 或使用 Homebrew (macOS)
brew install node@18

# 验证
node --version  # 需要 18+
npm --version
```

#### 3. Tauri CLI

```bash
# 使用 Cargo 安装
cargo install tauri-cli

# 验证
cargo tauri --version
```

#### 4. 平台特定依赖

**macOS:**
```bash
xcode-select --install
```

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

**Fedora:**
```bash
sudo dnf install webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel
```

**Windows:**
- 安装 [Visual Studio Build Tools 2019](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- 勾选 "C++ build tools" 和 "Windows 10 SDK"

---

## 项目结构

```
OxideTerm/
├── .github/                # GitHub Actions 配置
├── docs/                   # 项目文档
├── public/                 # 静态资源
│   ├── tauri.svg            # Tauri 入口页图标
│   └── fonts/              # 字体文件
├── src/                    # 前端源码
│   ├── components/         # React 组件
│   │   └── plugin/          # 插件 UI 视图
│   ├── hooks/              # 自定义 Hooks
│   ├── lib/                # 工具函数
│   │   └── plugin/          # 插件运行时与 UI Kit
│   ├── store/              # Zustand 状态
│   │   └── pluginStore.ts   # 插件运行时状态
│   ├── styles.css          # CSS 样式
│   └── types/              # TypeScript 类型
├── src-tauri/              # 后端源码
│   ├── capabilities/       # Tauri 权限配置
│   ├── icons/              # 应用图标
│   └── src/                # Rust 源码
├── package.json            # NPM 配置
├── tailwind.config.js      # Tailwind 配置
├── tsconfig.json           # TypeScript 配置
└── vite.config.ts          # Vite 配置
```

---

## 插件开发

- 插件 API 与生命周期说明见 [docs/PLUGIN_SYSTEM.md](PLUGIN_SYSTEM.md)
- 插件开发规范与 UI Kit 组件清单见 [docs/PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md)

---

## 开发工作流

### 启动开发服务器

```bash
# 安装 NPM 依赖
npm install

# 启动开发模式 (同时启动前端和后端)
npm run tauri dev
```

这会：
1. 启动 Vite 开发服务器 (热重载)
2. 编译 Rust 后端
3. 启动 Tauri 窗口

### 常用命令

```bash
# 仅前端开发 (不启动 Tauri)
npm run dev

# 仅检查 Rust 代码
cd src-tauri && cargo check

# 格式化代码
npm run lint          # 前端 (注意: lint 命令当前尚未配置)
cd src-tauri && cargo fmt  # 后端

# 构建生产版本
npm run tauri build
```

### 文件监听

开发模式下：
- **前端修改**: 自动热重载 (HMR)
- **Rust 修改**: 自动重新编译并重启应用
- **Tauri 配置修改**: 需要手动重启

---

## 代码规范

### TypeScript/React

```typescript
// 使用 函数组件 + Hooks
const MyComponent: React.FC<Props> = ({ prop1, prop2 }) => {
  const [state, setState] = useState<StateType>(initialValue);
  
  // 副作用使用 useEffect
  useEffect(() => {
    // ...
    return () => { /* cleanup */ };
  }, [dependencies]);
  
  return <div>...</div>;
};

// 使用 type 而非 interface (除非需要 extends)
type Props = {
  prop1: string;
  prop2?: number;
};

// 使用 cn() 合并 className
<div className={cn(
  "base-class",
  condition && "conditional-class"
)} />
```

### Rust

```rust
// 模块组织
mod submodule;
pub use submodule::*;

// 错误处理使用 thiserror
#[derive(Error, Debug)]
pub enum MyError {
    #[error("Failed to connect: {0}")]
    ConnectionFailed(String),
}

// 异步函数
pub async fn my_async_fn() -> Result<T, MyError> {
    let result = some_operation().await?;
    Ok(result)
}

// Tauri 命令
#[tauri::command]
pub async fn my_command(
    arg: String,
    state: State<'_, Arc<MyState>>,
) -> Result<Response, MyError> {
    // ...
}
```

### 命名约定

| 类型 | 约定 | 示例 |
|------|------|------|
| 文件名 (TS) | PascalCase (React 组件); camelCase (store/hooks/utils) | `MyComponent.tsx`, `useToast.ts` |
| 文件名 (Rust) | snake_case | `my_module.rs` |
| 组件名 | PascalCase | `TerminalView` |
| 函数名 | camelCase (TS) / snake_case (Rust) | `handleClick` / `handle_click` |
| 常量 | SCREAMING_SNAKE_CASE | `MAX_BUFFER_SIZE` |
| 类型 | PascalCase | `SessionInfo`, `ConnectRequest` |

---

## 测试

### 前端测试

> **注意**: 前端测试框架尚未配置，以下命令暂不可用。

```bash
# 运行测试
npm test

# 运行测试并生成覆盖率报告
npm test -- --coverage
```

### 后端测试

```bash
cd src-tauri

# 运行所有测试
cargo test

# 运行特定测试
cargo test test_name

# 带输出运行
cargo test -- --nocapture
```

### 集成测试

```bash
# 使用 Playwright 进行 E2E 测试
npm run test:e2e
```

---

## 调试技巧

### 前端调试

1. **浏览器开发者工具**
   - 开发模式下按 `Cmd+Option+I` (macOS) 或 `F12` (Windows/Linux)
   - 使用 Console、Network、Elements 面板

2. **React DevTools**
   - 安装浏览器扩展
   - 在 Tauri 窗口中使用

3. **日志输出**
   ```typescript
   console.log('Debug:', data);
   console.table(arrayData);
   ```

### 后端调试

1. **环境变量**
   ```bash
   # 启用详细日志
   RUST_LOG=debug npm run tauri dev
   
   # 更细粒度的日志
   RUST_LOG=oxideterm_lib=trace,russh=debug npm run tauri dev
   ```

2. **日志宏**
   ```rust
   use tracing::{info, debug, warn, error, trace};
   
   info!("Connection established: {}", session_id);
   debug!("Received data: {:?}", data);
   error!("Failed to connect: {}", e);
   ```

3. **VSCode 调试配置**
   
   `.vscode/launch.json`:
   ```json
   {
     "version": "0.2.0",
     "configurations": [
       {
         "type": "lldb",
         "request": "launch",
         "name": "Debug Tauri",
         "cargo": {
           "args": ["build", "--manifest-path=src-tauri/Cargo.toml"]
         },
         "args": []
       }
     ]
   }
   ```

### 网络调试

```bash
# 查看 WebSocket 连接
# 使用浏览器 Network 面板的 WS 标签

# 抓包分析
tcpdump -i lo0 port 22
wireshark
```

---

## 发布流程

### 版本号管理

遵循 [Semantic Versioning](https://semver.org/):
- `MAJOR.MINOR.PATCH`
- 例如: `1.0.0`, `1.1.0`, `1.1.1`

### 发布步骤

1. **更新版本号**
   ```bash
   # package.json
   npm version patch  # 或 minor / major
   
   # src-tauri/Cargo.toml
   # 手动更新 version 字段
   
   # src-tauri/tauri.conf.json
   # 手动更新 version 字段
   ```

2. **生成 Changelog**
   ```bash
   git log --oneline v1.0.0..HEAD
   ```

3. **构建发布版本**
   ```bash
   npm run tauri build
   ```

4. **创建 Git Tag**
   ```bash
   git tag -a v1.1.0 -m "Release v1.1.0"
   git push origin v1.1.0
   ```

### 构建产物

| 平台 | 产物位置 |
|------|---------|
| macOS | `src-tauri/target/release/bundle/dmg/` |
| Windows | `src-tauri/target/release/bundle/nsis/` |
| Linux | `src-tauri/target/release/bundle/deb/` |

---

## 贡献指南

### 提交 Issue

1. 搜索现有 Issues，避免重复
2. 使用 Issue 模板
3. 提供复现步骤和环境信息

### 提交 Pull Request

1. **Fork 仓库**
   ```bash
   git clone https://github.com/YOUR_USERNAME/OxideTerm.git
   ```

2. **创建分支**
   ```bash
   git checkout -b feature/your-feature
   # 或
   git checkout -b fix/your-fix
   ```

3. **编写代码**
   - 遵循代码规范
   - 添加必要的测试
   - 更新相关文档

4. **提交更改**
   ```bash
   git commit -m "feat: add amazing feature"
   # 遵循 Conventional Commits 规范
   # feat: 新功能
   # fix: 修复
   # docs: 文档
   # style: 格式
   # refactor: 重构
   # test: 测试
   # chore: 构建/工具
   ```

5. **推送并创建 PR**
   ```bash
   git push origin feature/your-feature
   ```

### 代码审查

- PR 需要至少一位维护者审核
- CI 检查必须通过
- 保持 PR 专注于单一功能/修复

---

## 常见问题

### Q: 编译时出现 "linking with cc failed"

**A:** 确保安装了构建工具:
```bash
# macOS
xcode-select --install

# Ubuntu
sudo apt install build-essential
```

### Q: 运行时出现 "WebGL not supported"

**A:** 确保显卡驱动是最新的，或尝试禁用 WebGL:
```typescript
// TerminalView.tsx
// term.loadAddon(new WebglAddon());  // 注释掉
term.loadAddon(new CanvasAddon());    // 使用 Canvas 替代
```

### Q: Windows 上构建速度很慢

**A:** 尝试:
1. 排除项目目录在杀毒软件扫描外
2. 使用 SSD
3. 增加内存

### Q: 如何连接需要跳板机的服务器

**A:** 目前支持 `~/.ssh/config` 的 ProxyJump 配置:
```
Host target
    HostName 10.0.0.1
    User admin
    ProxyJump bastion

Host bastion
    HostName 1.2.3.4
    User jump
```

---

## 联系方式

- **GitHub Issues**: 提交 Bug 和功能请求
- **Discussions**: 讨论和问答

感谢您的贡献! 🎉
