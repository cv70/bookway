# Bookway 商家中心

TypeScript + React（Vite）项目，覆盖商品、订单、库存和路线商品关联的日常经营流程。

运行 `npm install && npm run dev`。配置 `VITE_GATEWAY_URL` 后，商品、订单履约和
Affiliate 结算均通过 Gateway 的商家管理员接口读取；未配置网关时仅允许本地草稿预览，
不会被视为生产数据源。
