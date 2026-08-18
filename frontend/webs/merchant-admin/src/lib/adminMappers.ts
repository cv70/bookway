import { Product } from "../domain";
import {
  CreateMallProduct,
  MallProduct,
  MallSku,
  UpdateMallProduct,
} from "./adminApi";

const formatPrice = (cents: number) => `¥${(cents / 100).toFixed(2)}`;

const priceCents = (price: string) => {
  const amount = Number(price.replace(/[¥,]/g, ""));
  return Number.isFinite(amount) ? Math.round(amount * 100) : 0;
};

const kindFor = (attributes: Record<string, string>) =>
  attributes.kind === "课程" ? "课程" : "装备";

const warehouseFor = (
  attributes: Record<string, string>,
  kind: Product["kind"],
) =>
  kind === "课程" || attributes.warehouse === "数字内容库"
    ? "数字内容库"
    : "北京中心仓";

export function productFromMall(product: MallProduct, sku: MallSku): Product {
  const kind = kindFor(sku.attributes);
  return {
    id: product.id,
    name: product.title,
    spu: sku.attributes.spu || product.id,
    description: product.description,
    sku: sku.attributes.sku || sku.id,
    skuId: sku.id,
    variant: sku.attributes.variant || sku.title,
    kind,
    warehouse: warehouseFor(sku.attributes, kind),
    price: formatPrice(sku.price_cents),
    stock: Number(sku.attributes.display_stock || 0),
    sales: Number(sku.attributes.display_sales || 0),
    status: product.status === 1 && sku.saleable ? "销售中" : "待发布",
    image: product.image_url,
  };
}

export const productsFromMall = (items: MallProduct[]) =>
  items.flatMap((product) =>
    product.skus.map((sku) => productFromMall(product, sku)),
  );

const skuAttributes = (product: Product): Record<string, string> => ({
  spu: product.spu,
  sku: product.sku,
  variant: product.variant,
  kind: product.kind,
  warehouse: product.warehouse,
  // Inventory is owned by mall-inventory. These are only display projections
  // until its management contract is exposed through the gateway.
  display_stock: String(product.stock),
  display_sales: String(product.sales),
});

export function createMallProduct(product: Product): CreateMallProduct {
  return {
    title: product.name,
    description: product.description,
    image_url: product.image,
    status: product.status === "销售中" ? 1 : 0,
    skus: [
      {
        title: product.variant || product.name,
        price_cents: priceCents(product.price),
        currency: "CNY",
        attributes: skuAttributes(product),
        saleable: product.status === "销售中",
      },
    ],
  };
}

export function updateMallProduct(product: Product): UpdateMallProduct {
  return {
    title: product.name,
    description: product.description,
    image_url: product.image,
    status: product.status === "销售中" ? 1 : 0,
    sku_updates: [
      {
        sku_id: product.skuId,
        title: product.variant || product.name,
        price_cents: priceCents(product.price),
        currency: "CNY",
        attributes: { values: skuAttributes(product) },
        saleable: product.status === "销售中",
      },
    ],
  };
}
