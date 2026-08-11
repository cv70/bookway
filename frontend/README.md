# 万卷行 Frontend

万卷行客户端是独立 Expo 工程，使用 React Native 0.86、React 19、TypeScript strict、New Architecture、Fabric 和 Hermes，同一套代码支持 iOS、Android 与 Web 预览。

## 当前首版

- `今日`：行动清单、完成状态、专注时长和连续行动反馈。
- `发现`：多阶段推荐 Feed、领域筛选、统一搜索和后端不可用时的本地降级搜索。
- `社区互动`：发现页点赞已连接 Gateway，客户端即时维护点赞状态。
- `路线`：路线进度、下一行动和创建路线表单。
- `我的`：成长统计、回望、成就、隐私和设置入口。
- API 优先请求 Rust Gateway；本地服务不可用时使用同结构演示数据，不阻塞客户端开发。

## 目录

```text
frontend/
└── apps/mobile/
    ├── src/
    │   ├── api/          # Gateway HTTP client
    │   ├── analytics/    # 批量事件、曝光去重、重试与后台 flush
    │   ├── components/   # 通用业务组件
    │   ├── data/         # 离线演示数据
    │   ├── screens/      # 今日、发现、路线、我的
    │   ├── theme.ts      # 设计 token 与领域色
    │   └── types.ts      # 客户端契约类型
    ├── App.tsx
    ├── app.json
    ├── package.json
    └── package-lock.json
```

前端不引用 `../../backend` 中的 Rust 源码、配置或构建产物，只通过版本化 HTTPS API 通信。当前类型是首版手写镜像；OpenAPI 稳定后应由后端发布 artifact，并在前端独立生成 SDK。

## 本地运行

```bash
cd frontend/apps/mobile
npm install
npm run typecheck
npm run start
```

Web 预览：

```bash
npm run web
```

Gateway 默认地址：iOS/Web 使用 `http://127.0.0.1:8080`，Android 模拟器使用 `http://10.0.2.2:8080`。真机或自定义端口通过环境变量覆盖：

```bash
EXPO_PUBLIC_API_URL=http://192.168.1.10:8080 npm run start
```

生产或鉴权联调时通过 `EXPO_PUBLIC_AUTH_TOKEN` 提供短期 Bearer JWT；未设置时客户端仅在本地开发中发送 `demo-user` 身份。

当本机 `8080` 已被占用、后端按 `18080-18085` 启动时：

```bash
EXPO_PUBLIC_API_URL=http://127.0.0.1:18080 npm run web -- --port 19006
```

## 当前边界

- 本地开发身份仍是 `demo-user`；Feed/Search 已移除 `user_id` 查询参数，生产由 Bearer JWT 经 Gateway 注入可信身份。
- 曝光、点赞、搜索提交和行动完成已批量上报 `/v1/events`，支持 100 条 flush、定时 flush、后台 flush、重试和曝光去重。
- TypeScript API 类型目前是手写镜像；后端 OpenAPI 稳定后应生成并固定 SDK 版本。
- 搜索已能展示内容、用户和主题结果，但详情跳转、搜索历史、联想词面板和无限分页仍待实现。
- Feed 与搜索失败会使用演示数据，生产构建需要把降级状态接入可观测性并避免掩盖长时间后端故障。
- 发布、评论、收藏、关注、拉黑和举报仍需要完整移动端交互。
