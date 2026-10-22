<p align="center">
  <img src="web/public/brand/garage-ui.svg" width="80" alt="Garage UI 标志">
</p>

# Garage UI

[![Releases](https://img.shields.io/badge/Releases-GitHub-blue)](https://github.com/xjoker/garage-ui/releases)

**在一个浏览器里管理 Garage 集群、存储桶和文件。**

[English](README.md) | 简体中文

本文为英文版的辅助译本，内容以英文版为准。

[Garage](https://garagehq.deuxfleurs.fr/) 的非官方 Web 界面，支持浏览和上传文件、管理访问权限、查看集群健康状况。一个容器包含服务端和前端界面，无需额外的数据库。

## 快速开始：Docker Compose

选择下面一个示例，保存为 `compose.yaml`。直接使用已发布的镜像，无需克隆仓库或编译。

### 对接现有 Garage

将 Admin/S3 地址、管理令牌和 region 改成你的 Garage 配置：

```yaml
services:
  garage-ui:
    image: ghcr.io/xjoker/garage-ui:20261022.1
    restart: unless-stopped
    ports:
      - "127.0.0.1:3909:3909"
    environment:
      GARAGE_ADMIN_URL: http://192.0.2.10:3903
      GARAGE_ADMIN_TOKEN: replace-with-your-garage-admin-token
      S3_ENDPOINT_URL: http://192.0.2.10:3900
      S3_REGION: garage
      COOKIE_SECURE: "false"
    volumes:
      - garage-ui-data:/data
volumes:
  garage-ui-data:
```

地址必须从 UI 容器内可达；容器的 `localhost` 不是 Garage 服务器。如果把命名卷改为宿主机目录挂载，请加入下一节的 `garage-ui-init`，并让两个 UI 服务挂载同一个数据目录。管理令牌与 S3 访问密钥不同。Garage 位于同一宿主机或其他 Docker 网络时，见[容器网络说明](docs/wiki/Deployment.md#reach-your-existing-garage)。

### 一起部署 Garage 和 UI

将已有的 `garage.toml` 放在 `compose.yaml` 同目录，使用以下配置：

```yaml
x-logging: &default-logging
  driver: json-file
  options:
    max-size: "10m"
    max-file: "3"

services:
  garage:
    image: dxflrs/garage:v2.4.1
    restart: unless-stopped
    logging: *default-logging
    command: ["/garage", "server", "--single-node"]
    volumes:
      - ./garage.toml:/etc/garage.toml:ro
      - ./garage-data:/var/lib/garage
    networks:
      - garage

  garage-ui-init:
    image: alpine:3.22
    user: "0:0"
    network_mode: none
    command: ["sh", "-c", "chown 65532:65532 /data && chmod 0700 /data"]
    volumes:
      - ./garage-ui-data:/data

  garage-ui:
    image: ghcr.io/xjoker/garage-ui:20261022.1
    restart: unless-stopped
    logging: *default-logging
    depends_on:
      garage:
        condition: service_started
      garage-ui-init:
        condition: service_completed_successfully
    ports:
      - "127.0.0.1:3909:3909"
    environment:
      GARAGE_ADMIN_URL: http://garage:3903
      GARAGE_ADMIN_TOKEN: replace-with-your-garage-admin-token
      S3_ENDPOINT_URL: http://garage:3900
      S3_REGION: garage
      COOKIE_SECURE: "false"
      NO_PROXY: localhost,127.0.0.1,garage
    volumes:
      - ./garage-ui-data:/data
    networks:
      - garage

networks:
  garage:
    name: garage
```

`garage-ui-init` 只在启动阶段自动设置 UI 数据目录权限，随后 UI 以非 root 用户运行。两个 UI 服务必须挂载同一个专用数据目录。`NO_PROXY` 让内部 Garage 请求绕过继承的代理。UI 的管理令牌和 region 需与 `garage.toml` 一致。Garage 的元数据和数据路径应位于 `/var/lib/garage` 下，其他路径请相应调整卷挂载。全新安装可按 [Garage 快速入门](https://garagehq.deuxfleurs.fr/documentation/quick-start/)准备 `garage.toml`。

此示例用于新建**没有数据冗余的单节点 Garage**；已有集群使用上一节的 UI 配置即可。Garage API 仅在 Docker 网络内部开放，其他客户端需要访问时请单独发布 S3 服务。

### 启动并打开界面

```sh
docker compose up -d
```

访问 **[http://localhost:3909](http://localhost:3909)**，创建第一个 UI 管理员。创建存储桶和 S3 密钥，授予密钥访问该存储桶的权限，然后打开**对象（Objects）**上传文件。

示例使用本地 HTTP。远程访问请配置 [HTTPS](docs/wiki/Deployment.md#serve-the-ui-over-https)，并将 `COOKIE_SECURE` 改为 `"true"`。Docker 在远程服务器上时，可通过 `ssh -L 3909:127.0.0.1:3909 your-server` 访问本机端口。

## 管理部署

在 `compose.yaml` 所在目录中执行：

```sh
docker compose ps
docker compose logs --tail=100 garage-ui
docker compose down
```

`down` 会保留数据。加上 `--volumes` 会删除命名卷；绑定挂载的宿主机目录仍保留在磁盘上。

修改宿主机端口可直接调整 `ports` 映射；分享链接需要在 UI 服务的 `environment` 中配置接收者可访问的 `S3_PUBLIC_ENDPOINT_URL`。容器网络、HTTPS、备份、升级、不使用 Compose 的 Docker 部署、Linux 二进制和 Helm，见[部署指南](docs/wiki/Deployment.md)；完整配置项见[配置参考](docs/wiki/Configuration.md)。

## 功能

![Garage UI 对象浏览器](docs/images/objects-desktop.png)

<details>
<summary>在较小的屏幕上</summary>

<img src="docs/images/objects-mobile.png" width="240" alt="窄屏上的 Garage UI 对象浏览器">

</details>

- **文件：** 浏览文件夹，预览媒体和文本，上传文件或文件夹，打包下载 ZIP，创建限时分享链接。
- **存储：** 管理存储桶、别名、配额、CORS、生命周期规则，以及未完成的上传。
- **访问控制：** 管理 Garage 密钥和管理令牌；分配 UI 管理员或只读角色，分别授权下载和分享。
- **集群：** 查看健康状况、容量、节点、布局、工作线程和数据块错误，运行修复任务。
- **界面：** 浅色和深色主题、英文和简体中文、桌面和手机布局。

兼容性已在 **Garage v2.3.0 和 v2.4.1** 上测试。低于 v2.3.0 的版本无法通过 UI 执行写操作；更新版本需要单独验证。详情见[支持评估](docs/garage-support.md)。

## 安全与持久化

身份验证始终开启。首次访问创建 UI 管理员，Garage 管理令牌保留在服务端。请妥善保管 Compose 和凭据文件，远程访问使用 HTTPS。

UI 的 `/data` 卷保存账号、会话、审计记录、存储桶密钥选择和会话密钥。备份完整数据卷前请先停止 UI。实际对象和集群状态由 Garage 保存，请另行备份 Garage 的数据和配置，见[备份与恢复](docs/wiki/Deployment.md#back-up-and-restore-ui-state)。

发布的镜像和二进制没有签名。Release 包含 SBOM 和 SHA-256 校验和；校验和可验证文件完整性，不能验证发布者身份。

## 文档与贡献

以下文档均为英文：

- [使用指南](docs/wiki/Home.md)：首次使用、配置、存储、权限和集群操作。
- [故障排查](docs/wiki/Troubleshooting.md)：启动错误、登录循环，以及 Garage/S3 连接失败。
- [贡献指南](CONTRIBUTING.md)：源码构建和针对性检查。
- [支持评估](docs/garage-support.md)、[路线图](docs/roadmap.md)和[验证证据](docs/README.md#verification-evidence)：兼容性、交付状态和验证边界。
- [工作规格](docs/README.md)：需求和验收目标。

本项目与 [Noooste/garage-ui](https://github.com/Noooste/garage-ui) 是不同的项目。

## 许可证

[MIT](LICENSE)。
