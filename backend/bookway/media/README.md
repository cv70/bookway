# media 媒体服务

媒体服务负责对象存储和 CDN 的控制面：生成不可猜测的对象 key、校验 MIME 与大小、记录媒体元数据、返回上传和访问地址，并维护 `pending -> processing -> ready -> blocked/deleted` 生命周期。生产环境将 `S3_ENDPOINT` 指向 MinIO 或云厂商 S3 兼容接口，上传凭证由网关转发给客户端，媒体字节不经过业务服务。

## 接口

- `POST /v1/media/upload-url`：创建上传会话。
- `PUT /v1/media/{id}/upload`：本地代理上传（仅 `MEDIA_PROXY_UPLOAD=true` 时开启）。
- `POST /v1/media/{id}/complete`：通过对象存储 HEAD 校验实际大小和 MIME，再将资产置为 `processing`。
- `GET /v1/media/{id}`：读取元数据和 CDN 地址。
- 内部 gRPC `get_owned_ready_batch`：仅返回指定用户自己拥有、且已完成处理的资源；内容服务用它阻止外链、越权和未处理资源进入公开内容。

必须配置 `DATABASE_URL`、`S3_ENDPOINT`、`S3_BUCKET`、`S3_ACCESS_KEY`、`S3_SECRET_KEY` 和 `CDN_BASE_URL`。默认监听 `8091`。

Gateway 通过内部 gRPC `create_upload`、`complete_upload`、`get` 调用本服务。待上传资产只允许所有者读取；只有处理器验证对象在队列中完成并进入 `ready` 后才可作为公开 CDN 资源引用。

运行 `cargo run -p bookway-media-processor` 启动持续的处理器。它使用带 lease 的 PostgreSQL 工作队列再次校验对象大小、MIME 和允许类型；处理失败按指数退避重试，十次失败后会将资产标记为 `blocked`、任务标记为 `dead`。处理器是媒体审核链路的可靠门禁，生产部署应对 `media_processing_jobs.status = 'dead'` 告警，并在此门禁后接入图像病毒扫描、视频转码和派生图服务。
