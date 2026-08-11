# media 媒体服务

媒体服务负责对象存储和 CDN 的控制面：生成不可猜测的对象 key、校验 MIME 与大小、记录媒体元数据、返回上传和访问地址，并维护 `pending -> ready -> blocked/deleted` 生命周期。生产环境将 `S3_ENDPOINT` 指向 MinIO 或云厂商 S3 兼容接口，上传凭证由网关转发给客户端，媒体字节不经过业务服务。

## 接口

- `POST /v1/media/upload-url`：创建上传会话。
- `PUT /v1/media/{id}/upload`：本地代理上传（仅 `MEDIA_PROXY_UPLOAD=true` 时开启）。
- `POST /v1/media/{id}/complete`：通过对象存储 HEAD 校验实际大小和 MIME，再将资产置为 ready。
- `GET /v1/media/{id}`：读取元数据和 CDN 地址。

必须配置 `DATABASE_URL`、`S3_ENDPOINT`、`S3_BUCKET`、`S3_ACCESS_KEY`、`S3_SECRET_KEY` 和 `CDN_BASE_URL`。默认监听 `8091`。

Gateway 使用对应的 `/internal/v1/media/*` 路由调用本服务。待上传资产只允许所有者读取；完成并进入 `ready` 后才可作为公开 CDN 资源引用。图片处理、视频转码、病毒扫描和多媒体审核仍属于下一阶段。
