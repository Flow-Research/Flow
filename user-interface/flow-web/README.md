# Flow Web

A modern React application built with Vite for the Flow project, featuring both web and desktop capabilities.

## Features

- ⚡️ **Vite** - Lightning-fast development and building
- ⚛️ **React 18** - Modern React with JSX and hooks
- 🖥️ **Electron** - Cross-platform desktop application
- 🎨 **Modern UI** - Beautiful gradients, animations, and responsive design
- 🔧 **ESLint** - Code quality and consistency
- 🚀 **Hot Module Replacement** - Instant development feedback
- 🔒 **Secure Architecture** - Context isolation and secure IPC
- 📦 **Cross-Platform Packaging** - Build for macOS, Windows, and Linux

## Getting Started

### Prerequisites

- **Node.js** (version 16 or higher)
- **npm** or **yarn** package manager
- For desktop builds: Platform-specific build tools (automatically handled by electron-builder)

### Quick Start

1. **Navigate to the project directory:**

   ```bash
   cd user-interface/flow-web
   ```

2. **Install dependencies:**

   ```bash
   npm install
   ```

3. **Choose your development mode:**

   **Web Development:**

   ```bash
   npm run dev
   ```

   Then open `http://localhost:3000` in your browser.

   **Desktop Development:**

   ```bash
   npm run electron-dev
   ```

   This starts both the web server and Electron app simultaneously.

## Available Scripts

### 🌐 Web Development

| Command           | Description                         |
| ----------------- | ----------------------------------- |
| `npm run dev`     | Start development server (web only) |
| `npm run build`   | Build for production                |
| `npm run preview` | Preview production build locally    |
| `npm run lint`    | Run ESLint for code quality         |

### 🖥️ Desktop Application (Electron)

| Command                 | Description                                       |
| ----------------------- | ------------------------------------------------- |
| `npm run electron-dev`  | Start both Vite dev server and Electron app       |
| `npm run electron`      | Start Electron only (requires dev server running) |
| `npm run electron-pack` | Build and package for all platforms               |

### 🔧 Development Tools

| Command                         | Description                                   |
| ------------------------------- | --------------------------------------------- |
| `node scripts/test-electron.js` | Test Electron setup (builds and runs briefly) |

## 🖥️ Desktop Application Features

Flow Web seamlessly runs as both a web and desktop application using Electron.

### 🚀 Desktop Capabilities

- **🎛️ Native Menu Bar** - Full application menu with keyboard shortcuts
- **🪟 Window Management** - Minimize, maximize, close, and resize controls
- **🔒 Secure Architecture** - Context isolation prevents security vulnerabilities
- **💾 File System Access** - Read/write capabilities (when implemented)
- **🌐 Offline Mode** - Works without internet connection
- **⚡ Platform Integration** - Native notifications and system tray (coming soon)

### 📦 Supported Platforms

| Platform    | Package Format          | Architectures              |
| ----------- | ----------------------- | -------------------------- |
| **macOS**   | `.dmg`                  | x64, ARM64 (Apple Silicon) |
| **Windows** | `.exe` (NSIS installer) | x64                        |
| **Linux**   | `.AppImage`             | x64                        |

### 🔧 Desktop Development

**Development Mode:**

```bash
npm run electron-dev
```

- Starts Vite dev server on `http://localhost:3000`
- Launches Electron app automatically
- Hot reload for both renderer and main process

**Production Build:**

```bash
npm run electron-pack
```

- Builds optimized web assets
- Packages into platform-specific installers
- Output directory: `electron-dist/`

## 📁 Project Structure

```
flow-web/
├── 📁 public/                    # Static assets and Electron files
│   ├── electron.cjs             # 🖥️ Main Electron process
│   ├── preload.cjs             # 🔒 Secure IPC communication
│   └── vite.svg                # 🎨 Vite logo
├── 📁 src/                      # React application source
│   ├── 📁 components/
│   │   ├── FlowLogo.jsx        # 🎨 Animated Flow logo
│   │   ├── FlowLogo.css
│   │   ├── ElectronInfo.jsx    # 📊 Platform detection & info
│   │   └── ElectronInfo.css
│   ├── App.jsx                 # 🏠 Main application component
│   ├── App.css                 # 🎨 Global application styles
│   ├── main.jsx                # ⚛️ React entry point
│   └── index.css               # 🎨 Base styles and theme
├── 📁 scripts/                  # Development utilities
│   ├── electron-dev.js         # 🔧 Development server script
│   └── test-electron.js        # ✅ Electron testing script
├── 📁 dist/                     # Built web assets (generated)
├── 📁 electron-dist/            # Packaged desktop apps (generated)
├── index.html                  # 🌐 HTML entry point
├── package.json                # 📦 Dependencies and scripts
├── vite.config.js              # ⚡ Vite configuration
├── .eslintrc.cjs               # 🔧 ESLint configuration
├── .gitignore                  # 📝 Git ignore rules
└── README.md                   # 📖 This file
```

## 🛠️ Technology Stack

### Core Technologies

- **⚛️ React 18** - Modern JavaScript UI library with hooks and concurrent features
- **⚡ Vite** - Next-generation frontend build tool with instant HMR
- **🖥️ Electron** - Cross-platform desktop app framework using web technologies
- **🔧 ESLint** - JavaScript/JSX linting and code quality enforcement

### Dependencies

- **📦 electron-builder** - Complete solution to package Electron apps
- **🔄 concurrently** - Run multiple npm scripts simultaneously
- **⏳ wait-on** - Wait for services to be available before starting
- **🔍 electron-is-dev** - Detect if Electron is running in development
- **🚀 electron-squirrel-startup** - Handle Squirrel events on Windows

### Development Features

- **🎨 Modern CSS** - Gradients, animations, and responsive design
- **🔒 Secure IPC** - Context isolation and preload scripts
- **📱 Responsive Design** - Works on desktop and web browsers
- **🎯 Hot Reload** - Instant feedback during development

## 🤝 Contributing

We welcome contributions to Flow Web! Please read the main project's [CONTRIBUTING.md](../../CONTRIBUTING.md) for details on:

- Code of conduct
- Development workflow
- Pull request process
- Coding standards

### Development Workflow

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes and test them:
   ```bash
   npm run lint                 # Check code quality
   npm run build               # Test production build
   npm run electron-dev        # Test desktop functionality
   ```
4. Commit your changes: `git commit -m 'Add amazing feature'`
5. Push to the branch: `git push origin feature/amazing-feature`
6. Open a Pull Request

## 📄 License

This project is part of the Flow ecosystem and follows the same licensing terms. See the [LICENSE](../../LICENSE) file in the root directory for details.

---

<div align="center">

**Flow Web** - Building the future of collaborative knowledge management

[🌐 Web Demo](http://localhost:3000) • [📖 Documentation](../../specs/) • [🐛 Report Bug](../../issues) • [💡 Request Feature](../../issues)

## NX Setup

# Individual project commands

nx run flow-web:dev # Start web dev server
nx run flow-web:electron-dev # Start desktop app development

# Aggregate commands

nx run user-interface:dev-web # Convenience alias for web dev
nx run user-interface:dev-desktop # Convenience alias for desktop dev
nx run user-interface:lint-all # Lint both projects
nx run user-interface:install-all # Install deps for both projects
nx run user-interface:build-all # Build everything

# Build and package

nx run flow-web:build # Build web assets
nx run flow-web:electron-pack # Package desktop app

</div>
