# 后台消费者

- `community-notification-dispatcher/`：领取 Gateway 已解析接收者的社区通知任务，使用租约、指数退避和终态 `dead` 状态可靠投递到 Growth 私有收件箱；Growth 的稳定来源键使确认前崩溃后的重放不产生重复通知。
