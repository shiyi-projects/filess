# AI 文件整理工具

## 目录结构

1. `dev_docs/`
   正式开发文档与约束源。
2. `apps/desktop/`
   Tauri + React + TypeScript 桌面应用。
3. `services/sidecar/`
   Python 解析与 AI sidecar。
4. `scripts/`
   统一放置后续自动化脚本，避免脚本散落。

## 开发约束

1. 任何影响架构、接口、数据模型、UI 主流程或业务规则的改动，先更新 `dev_docs/`。
2. 文件系统写操作只能由 Rust 侧发起。
3. Sidecar 不直接控制物理路径。
4. 搜索范围仅限已纳入本工具管理的文件。

## 启动顺序

1. 先安装前端依赖：`npm.cmd install`
2. 进入 `apps/desktop/` 运行桌面端：`npm.cmd run tauri:dev`
3. Sidecar 由桌面端后续统一拉起；当前仓库保留独立 Python 骨架，便于单测和协议开发。

## 许可证（License）

本项目采用 **[PolyForm Noncommercial License 1.0.0](./LICENSE)**（源代码可见、非商业用途免费）。

- **个人与非商业用途免费**：个人学习、研究、实验、业余/爱好项目，以及慈善、教育、科研、公共安全/健康、环保、政府等非商业组织，均可免费使用、修改与分发，但须保留 `LICENSE` 中的版权声明与 `Required Notice`。
- **商业用途需事先授权**：任何商业用途（包括但不限于将本软件用于盈利产品/服务、企业内部生产环境、对外提供付费或商业化服务等）**必须事先获得书面商业授权**。
- **商业授权联系方式**：`shiyi0x7f@gmail.com`（请注明用途与使用规模）。

> 不确定自己的场景是否属于商业用途时，请先邮件咨询，以免违反许可证条款。

## 贡献与 Pull Request

欢迎提交 Issue 与 Pull Request。提交前请阅读根目录 `CLAUDE.md` 中的工程约定（分支、提交、格式化、测试等）。

### 分支命名

- 格式：`<type>/<short-description>`
- type 取值：`feat` / `fix` / `refactor` / `chore` / `docs` / `test`
- 示例：`feat/add-batch-rename`、`fix/sidecar-timeout`

### Commit Message（Conventional Commits）

- 格式：`<type>: <简短描述>`
- type 与分支前缀一致；type 用英文，描述可用中文
- 一次提交只做一件事（原子提交）
- 示例：`fix: 修复重新分类时的数据丢失`

### PR 标题与正文格式

- **标题**：沿用 Conventional Commits，`<type>: <简短描述>`
- **正文**建议包含以下小节：

```markdown
## 变更说明
<!-- 这个 PR 做了什么、为什么 -->

## 关联 Issue
Closes #<issue-id>

## 变更类型
- [ ] feat（新功能）
- [ ] fix（缺陷修复）
- [ ] refactor（重构）
- [ ] docs（文档）
- [ ] chore / test（杂项 / 测试）

## 自测清单
- [ ] 本地通过格式化与 lint（rustfmt + clippy / prettier + eslint / ruff）
- [ ] 相关单测/集成测试通过（`npm run desktop:check`、`npm run sidecar:test`）
- [ ] 较大改动已同步更新 `dev_docs/`
- [ ] 不含密钥/令牌等敏感信息

## 截图 / 录屏（涉及 UI 时）
<!-- 可选 -->
```

### 提交前自检

1. 仅对本次改动涉及的文件做格式化与 lint，不顺手重构无关代码。
2. 桌面端检查：`npm run desktop:check`；Sidecar 测试：`npm run sidecar:test`。
3. 确认 `dev_docs/` 已就较大改动同步更新。

> 提交贡献即表示你同意：你的贡献以本项目的 PolyForm Noncommercial License 1.0.0 授权合并，且商业授权权利归项目版权人所有。

