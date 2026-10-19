import { afterEach, expect, test, vi } from "vitest";
import { advanceAuthGeneration, api, bindUnauthenticated, captureAuthGeneration, noteUnauthenticated, putObject } from "./client";

afterEach(() => {
  bindUnauthenticated(null);
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

test("only an unauthenticated 401 from the current session clears authentication", () => {
  const clear = vi.fn();
  bindUnauthenticated(clear);
  const generation = captureAuthGeneration();
  noteUnauthenticated(403, "unauthenticated", generation);
  noteUnauthenticated(401, "invalid_credentials", generation);
  expect(clear).not.toHaveBeenCalled();
  noteUnauthenticated(401, "unauthenticated", generation);
  expect(clear).toHaveBeenCalledOnce();
  noteUnauthenticated(401, "unauthenticated", generation);
  expect(clear).toHaveBeenCalledOnce();
  noteUnauthenticated(401, "unauthenticated", captureAuthGeneration());
  expect(clear).toHaveBeenCalledTimes(2);
});

test("a delayed API error still rejects without clearing a replacement session", async () => {
  let finishBody!: (text: string) => void;
  const body = new Promise<string>((resolve) => { finishBody = resolve; });
  const response = new Response(null, { status: 401 });
  vi.spyOn(response, "text").mockImplementation(() => body);
  vi.stubGlobal("fetch", vi.fn(async () => response));
  const clear = vi.fn();
  bindUnauthenticated(clear);
  const result = api("/api/keys").catch((error: unknown) => error);
  advanceAuthGeneration();
  finishBody(JSON.stringify({ code: "unauthenticated", message: "Login required" }));
  expect(await result).toMatchObject({ code: "unauthenticated", message: "Login required" });
  expect(clear).not.toHaveBeenCalled();
});

test("uploads capture their session before send and ignore an older session's 401", async () => {
  const requests: UploadRequest[] = [];
  class UploadRequest {
    status = 401;
    responseText = JSON.stringify({ code: "unauthenticated", message: "Login required" });
    withCredentials = false;
    upload = { onprogress: null };
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    onabort: (() => void) | null = null;
    constructor() { requests.push(this); }
    open() {}
    setRequestHeader() {}
    send() {}
    abort() { this.onabort?.(); }
  }
  vi.stubGlobal("XMLHttpRequest", UploadRequest);
  const clear = vi.fn();
  bindUnauthenticated(clear);
  const input = { bucketId: "bucket-a", key: "file.txt", body: new Blob(["body"]), contentType: "text/plain", onProgress: vi.fn() };
  const old = putObject(input).promise.catch((error: unknown) => error);
  advanceAuthGeneration();
  requests[0].onload?.();
  expect(await old).toBeInstanceOf(Error);
  expect(clear).not.toHaveBeenCalled();
  const current = putObject(input).promise.catch((error: unknown) => error);
  requests[1].onload?.();
  expect(await current).toBeInstanceOf(Error);
  expect(clear).toHaveBeenCalledOnce();
});

test("a viewer's refused change shows the read-only message instead of a bare status", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response(
    JSON.stringify({ code: "viewer_read_only", message: "server text" }),
    { status: 403 },
  )));
  const refused = await api("/api/buckets/b1/object?key=a.txt", { method: "DELETE" }).catch((error: unknown) => error);
  expect(refused).toMatchObject({
    code: "viewer_read_only",
    message: "Viewer accounts are read-only. The access key bound to this account cannot be used to upload, change, or delete objects.",
  });

  class UploadRequest {
    status = 403;
    responseText = JSON.stringify({ code: "viewer_read_only", message: "server text" });
    withCredentials = false;
    upload = { onprogress: null };
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    onabort: (() => void) | null = null;
    open() {}
    setRequestHeader() {}
    send() { this.onload?.(); }
    abort() { this.onabort?.(); }
  }
  vi.stubGlobal("XMLHttpRequest", UploadRequest);
  const upload = await putObject({ bucketId: "b1", key: "up.txt", body: new Blob(["x"]), contentType: "text/plain", onProgress: vi.fn() })
    .promise.catch((error: unknown) => error);
  expect(upload).toMatchObject({ code: "viewer_read_only" });
  expect((upload as Error).message).toContain("Viewer accounts are read-only.");
});

test("binding a viewer to a writable key shows why the key was refused", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response(
    JSON.stringify({ code: "key_exceeds_role", message: "server text" }),
    { status: 403 },
  )));
  const refused = await api("/api/users/2", { method: "PATCH", body: JSON.stringify({ access_key_id: "GKwriter" }) })
    .catch((error: unknown) => error);
  expect(refused).toMatchObject({
    code: "key_exceeds_role",
    message: "A viewer can be bound only to a read-only key. This key has write or owner permission on at least one bucket.",
  });
});

test("other refusals keep the server's message", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response(
    JSON.stringify({ code: "forbidden", message: "Not allowed" }),
    { status: 403 },
  )));
  const refused = await api("/api/keys").catch((error: unknown) => error);
  expect(refused).toMatchObject({ code: "forbidden", message: "Not allowed" });
});
