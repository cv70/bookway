# Media 后台处理器

- `media-processor/`：使用 PostgreSQL 租约队列复核已完成上传对象的大小、MIME 和允许类型；成功后将资产从 `processing` 转为 `ready`，失败按指数退避重试，十次失败后将资产置为 `blocked`、任务置为 `dead`。

运行：

```sh
cargo run -p bookway-media-processor
```

它与 `media` 服务使用相同的 `DATABASE_URL`、`S3_ENDPOINT`、`S3_BUCKET`、`S3_REGION`、`S3_ACCESS_KEY` 和 `S3_SECRET_KEY`。生产环境需监控 `media_processing_jobs.status = 'dead'`。
