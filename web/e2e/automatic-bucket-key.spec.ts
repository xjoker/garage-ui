import { expect, test } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { administrator } from "./accounts";
import { submitLogin } from "./login";

test("automatic bucket keys never widen a key-bound viewer's permissions", async ({ page, browser }) => {
  await page.goto("/");
  await page.getByLabel("Username").fill(administrator.username);
  await page.getByLabel("Password").fill(administrator.password);
  expect((await submitLogin(page)).status()).toBe(200);
  const csrf = (await page.context().cookies()).find(cookie => cookie.name === "garage_csrf")?.value;
  if (!csrf) throw new Error("Administrator CSRF cookie is missing");
  const headers = { "x-csrf-token": csrf };
  const suffix = randomUUID();
  const buckets: string[] = [];
  const keys: string[] = [];
  let userId: number | null = null;
  const tenantContext = await browser.newContext({ baseURL: new URL(page.url()).origin });
  try {
    for (const alias of [`auto-key-first-${suffix}`, `auto-key-second-${suffix}`]) {
      const response = await page.request.post("/api/buckets", { headers, data: { globalAlias: alias } });
      expect(response.status()).toBe(200);
      buckets.push((await response.json() as { id: string }).id);
    }
    for (const name of [`auto-admin-${suffix}`, `auto-tenant-${suffix}`]) {
      const response = await page.request.post("/api/keys", { headers, data: { name } });
      expect(response.status()).toBe(200);
      keys.push((await response.json() as { accessKeyId: string }).accessKeyId);
    }
    for (const bucket of buckets) {
      expect((await page.request.post(`/api/buckets/${bucket}/permissions`, {
        headers, data: { accessKeyId: keys[0], permissions: { read: true, write: true, owner: false } },
      })).status()).toBe(200);
      // No /credential call: the administrator must discover an existing grant automatically.
      expect((await page.request.get(`/api/buckets/${bucket}/objects`)).status()).toBe(200);
    }
    expect((await page.request.post(`/api/buckets/${buckets[0]}/permissions`, {
      headers, data: { accessKeyId: keys[1], permissions: { read: true, write: false, owner: false } },
    })).status()).toBe(200);
    const username = `Tenant-${suffix}`;
    const password = randomUUID();
    const created = await page.request.post("/api/users", {
      headers, data: { username, password, role: "viewer", can_download: true, access_key_id: keys[1] },
    });
    expect(created.status()).toBe(201);
    userId = (await created.json() as { id: number }).id;
    const tenant = await tenantContext.newPage();
    await tenant.goto("/");
    await tenant.getByLabel("Username").fill(username);
    await tenant.getByLabel("Password").fill(password);
    expect((await submitLogin(tenant)).status()).toBe(200);
    const visible = await tenant.request.get("/api/buckets");
    expect(visible.status()).toBe(200);
    expect((await visible.json() as { items: { id: string }[] }).items.map(bucket => bucket.id)).toEqual([buckets[0]]);
    expect((await tenant.request.get(`/api/buckets/${buckets[0]}/objects`)).status()).toBe(200);
    expect((await tenant.request.get(`/api/buckets/${buckets[1]}/objects`)).status()).toBe(404);
    const tenantCsrf = (await tenantContext.cookies()).find(cookie => cookie.name === "garage_csrf")?.value;
    if (!tenantCsrf) throw new Error("Tenant CSRF cookie is missing");
    expect((await tenant.request.put(`/api/buckets/${buckets[0]}/object?key=forbidden.txt`, {
      headers: { "x-csrf-token": tenantCsrf, "content-type": "text/plain" }, data: "forbidden",
    })).status()).toBe(403);
    expect((await page.request.delete(`/api/buckets/${buckets[0]}/permissions`, {
      headers, data: { accessKeyId: keys[1], permissions: { read: true, write: false, owner: false } },
    })).status()).toBe(200);
    expect((await tenant.request.get(`/api/buckets/${buckets[0]}/objects`)).status()).toBe(404);
    expect((await page.request.get(`/api/buckets/${buckets[0]}/objects`)).status()).toBe(200);
  } finally {
    await tenantContext.close();
    if (userId !== null) expect((await page.request.delete(`/api/users/${userId}`, { headers })).status()).toBe(204);
    for (const bucket of buckets) expect((await page.request.delete(`/api/buckets/${bucket}`, { headers })).status()).toBe(200);
    for (const key of keys) expect((await page.request.delete(`/api/keys/${key}`, { headers })).status()).toBe(200);
  }
});
