<p align="center">
  <img src="web/public/brand/garage-ui.svg" width="80" alt="Garage UI 标志">
</p>

# Garage UI

[![Releases](https://img.shields.io/badge/Releases-GitHub-blue)](https://github.com/xjoker/garage-ui/releases)

**在一个浏览器里管理 Garage 集群、存储桶和文件。**

[English](README.md) | 简体中文

本文为英文版的辅助译本，内容以英文版为准。

[Garage](https://garagehq.deuxfleurs.fr/) 的非官方 Web 界面，支持浏览和上传文件、管理访问权限、查看集群健康状况。一个容器包含服务端和前端界面，无需额外的数据库。

## Docker Compose

将下面内容保存为 `compose.yaml`，替换四项 Garage 配置后运行 `docker compose up -d`。直接使用已发布的镜像，无需克隆仓库、编译或 `.env` 文件。

```yaml
services:
  garage-ui:
    image: ghcr.io/xjoker/garage-ui:20261023.4
    restart: unless-stopped
    ports:
      - "127.0.0.1:3909:3909"
    environment:
      GARAGE_ADMIN_URL: http://garage:3903
      GARAGE_ADMIN_TOKEN: replace-with-your-garage-admin-token
      S3_ENDPOINT_URL: http://garage:3900
      S3_REGION: garage
      COOKIE_SECURE: "false"
    volumes:
      - garage-ui-data:/data
volumes:
  garage-ui-data:
```

示例里的 `garage` 是服务名。若 Garage 不在这个 Compose 项目中，填写 UI 容器能访问的地址。管理令牌来自 Garage 配置，不能用 S3 Key 代替。命名卷会自动为非 root 的 UI 准备好权限；如改用宿主机目录，请将其所有者设为 UID/GID `65532:65532`，并只供 UI 使用。

### 一起部署 Garage 和 UI

新建单节点 Garage 时，将自己的 `garage.toml` 放在 `compose.yaml` 旁边，然后使用：

```yaml
services:
  garage:
    image: dxflrs/garage:v2.4.1
    restart: unless-stopped
    command: ["/garage", "server", "--single-node"]
    volumes:
      - ./garage.toml:/etc/garage.toml:ro
      - garage-data:/var/lib/garage

  garage-ui:
    image: ghcr.io/xjoker/garage-ui:20261023.4
    restart: unless-stopped
    depends_on:
      - garage
    ports:
      - "127.0.0.1:3909:3909"
    environment:
      GARAGE_ADMIN_URL: http://garage:3903
      GARAGE_ADMIN_TOKEN: replace-with-your-garage-admin-token
      S3_ENDPOINT_URL: http://garage:3900
      S3_REGION: garage
      COOKIE_SECURE: "false"
    volumes:
      - garage-ui-data:/data

volumes:
  garage-data:
  garage-ui-data:
```

UI 的令牌和区域须与 `garage.toml` 一致。Garage 的 Admin API 应监听 `0.0.0.0:3903`，S3 应监听 `0.0.0.0:3900`；数据与元数据路径须放在 `/var/lib/garage` 下，或相应调整挂载。配置方法见 Garage 的[快速开始](https://garagehq.deuxfleurs.fr/documentation/quick-start/)。此示例只有一个节点，不提供数据冗余。Garage API 仅在 Compose 网络内部开放；其他客户端需要访问 S3 时，另行映射 S3 端口。

### 启动并打开界面

```sh
docker compose up -d
```

访问 **[http://localhost:3909](http://localhost:3909)**，创建第一个 UI 管理员。UI 有独立的登录账号和密码，在此页面自行设置；Garage 管理令牌和 S3 Key 不是 UI 登录凭据。

已有存储桶会自动选择兼容的读写 S3 Key。新建存储桶时，创建或选择 Key 并授予访问权限。普通用户仍为只读；绑定 S3 Key 的账号继续受该 Key 的授权限制，下载和分享权限分别控制。

示例使用本地 HTTP。远程访问请配置 [HTTPS](docs/wiki/Deployment.md#serve-the-ui-over-https)，并将 `COOKIE_SECURE` 改为 `"true"`。Docker 在远程服务器上时，可通过 `ssh -L 3909:127.0.0.1:3909 your-server` 访问本机端口。

## 管理部署

在 `compose.yaml` 所在目录中执行：

```sh
docker compose ps
docker compose logs --tail=100 garage-ui
docker compose down
```

`down` 会保留数据。加上 `--volumes` 会删除命名卷；绑定挂载的宿主机目录仍保留在磁盘上。

远程 HTTPS 访问时，将 `COOKIE_SECURE` 设为 `"true"`，并在 loopback 端口前配置反向代理。分享链接需要为 `S3_PUBLIC_ENDPOINT_URL` 配置接收者可访问的地址。容器网络、备份、升级、二进制和 Helm 见[部署指南](docs/wiki/Deployment.md)，完整配置项见[配置参考](docs/wiki/Configuration.md)。

## 功能

- **文件：** 浏览文件夹，预览媒体和文本，上传文件或文件夹，打包下载 ZIP，创建限时分享链接。
- **存储：** 管理存储桶、别名、配额、CORS、生命周期规则，以及未完成的上传。
- **访问控制：** 管理 Garage 密钥和管理令牌；分配 UI 管理员或只读角色，分别授权下载和分享。
- **集群：** 查看健康状况、容量、节点、布局、工作线程和数据块错误，运行修复任务。
- **界面：** 浅色和深色主题、英文和简体中文、桌面和手机布局。

兼容性已在 **Garage v2.3.0 和 v2.4.1** 上测试。低于 v2.3.0 的版本无法通过 UI 执行写操作；更新版本需要单独验证。详情见[支持评估](docs/garage-support.md)。

## 安全与持久化

身份验证始终开启。首次访问创建 UI 管理员，Garage 管理令牌保留在服务端。包含令牌的 Compose 文件请妥善保管，远程访问使用 HTTPS。

UI 的 `/data` 卷保存账号、会话、审计记录、存储桶密钥选择和会话密钥。备份完整数据卷前请先停止 UI。实际对象和集群状态由 Garage 保存，请另行备份 Garage 的数据和配置，见[备份与恢复](docs/wiki/Deployment.md#back-up-and-restore-ui-state)。

发布的镜像和二进制没有签名。Release 包含 SBOM 和 SHA-256 校验和；校验和可验证文件完整性，不能验证发布者身份。

## 文档与贡献

以下文档均为英文：

- [使用指南](docs/wiki/Home.md)：首次使用、配置、存储、权限和集群操作。
- [故障排查](docs/wiki/Troubleshooting.md)：启动错误、登录循环，以及 Garage/S3 连接失败。
- [贡献指南](CONTRIBUTING.md)：源码构建和针对性检查。
- [支持评估](docs/garage-support.md)、[路线图](docs/roadmap.md)和[验证证据](docs/README.md#verification-evidence)：兼容性、交付状态和验证边界。

本项目与 [Noooste/garage-ui](https://github.com/Noooste/garage-ui) 是不同的项目。

## 许可证

[MIT](LICENSE)。
