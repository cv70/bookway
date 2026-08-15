# Bookway

## 微服务组织结构

每个微服务位于 `backend/bookway/<service-name>/`。服务根目录是该服务的
Rust 主包；其 `src/` 目录承载长期运行的服务主体。除非任务属于下面定义的
独立执行单元，否则实现应放在服务主体中。

```text
backend/bookway/<service-name>/
├── Cargo.toml              # 服务主体 Rust 包
├── README.md               # 服务职责、依赖和运行说明
├── api/                    # 对外暴露的契约 Rust 包
│   ├── Cargo.toml
│   └── src/
├── bg/                     # 可选：持续运行的后台任务
│   └── <task-name>/
│       ├── Cargo.toml
│       └── src/main.rs
├── cronjob/                # 可选：由调度器周期触发的任务
│   └── <task-name>/
│       ├── Cargo.toml
│       └── src/main.rs
├── job/                    # 可选：按需执行并退出的一次性任务
│   └── <task-name>/
│       ├── Cargo.toml
│       └── src/main.rs
└── src/                    # 服务主体实现
    ├── main.rs
    ├── api/
    ├── conf/
    ├── datasource/
    └── domain/
```

### 根目录与 `api/`

- 根目录的 `Cargo.toml` 和 `src/` 构成微服务的主可执行程序，负责启动、
  配置装配和对外服务。
- `api/` 是独立 Rust 包，仅存放该服务对其他服务暴露的稳定契约，例如
  gRPC/Protobuf 定义、生成代码，以及客户端所需的公共类型。
- 不要将服务私有领域逻辑、数据访问实现或运行时配置放入 `api/`；调用方只能
  通过契约依赖服务，而不能依赖其 `src/` 实现。
- Protobuf 是服务间 RPC 的唯一数据契约。`.proto` 必须直接定义请求、响应、
  枚举和嵌套消息的业务字段；禁止使用 `request_json`、`response_json`、
  `JsonRequest` 或 `JsonResponse` 之类的 JSON 字符串信封。
- 业务代码直接使用生成的 `pb::<Method>Request`、`pb::<Method>Response` 和
  tonic 生成的 Client。不要为服务间 RPC 另建 `XxxRequest`、`XxxResponse` 或
  `XxxDto` schema，也不要为生成 Client 再封装一层转发 Client。

### 独立任务目录

- `bg/<task-name>/`：持续运行的后台 Rust 二进制项目，例如消费者、索引器或
  异步处理器；每个任务拥有自己的 `Cargo.toml` 和 `src/main.rs`。
- `cronjob/<task-name>/`：由外部调度器按固定周期启动的 Rust 二进制项目；任务
  完成一次调度工作后退出。
- `job/<task-name>/`：按发布、运维或人工触发的一次性 Rust 二进制项目；完成其
  明确的有限工作后退出。
- 新增任何 `bg`、`cronjob` 或 `job` 子项目时，都必须在 `backend/Cargo.toml` 的
  `workspace.members` 中登记，保证可由工作区统一构建、检查和测试。

### `src/` 服务主体

- `main.rs`：进程入口。初始化日志和运行时，读取配置，组装依赖并启动服务。
- `api/`：传输层适配器，例如 HTTP/gRPC 路由、处理器和服务实现。这里负责把
  外部请求转换为领域调用，不承载核心业务规则。
- `conf/`：服务私有的配置模型、环境变量解析和配置校验。
- `datasource/`：数据库、缓存、消息系统及其他非微服务依赖的访问实现；不要在
  此处包装其他微服务的 gRPC Client。
- `domain/`：核心业务用例、领域模型和规则；它直接持有并调用所依赖服务的 tonic
  生成 Client，使用对应 API 包的 Protobuf 类型。

若服务有额外的职责，可在 `src/` 下新增命名清晰的模块；依赖方向应保持为
`api -> domain -> datasource`；跨服务 gRPC Client 由 `main.rs` 组装后交给
`domain`，并由 `domain` 直接调用。
