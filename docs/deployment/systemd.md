# systemd 部署

::: tip 这一篇适合谁
云主机、物理机、长期稳定运行的 Linux 服务器。
systemd 只负责拉起进程，HTTPS / 域名 / WebDAV 透传全部交给前面的反向代理——请勿让 `aster_drive` 直接暴露到公网。
:::

systemd 适合长期稳定运行的 Linux 服务器。
这类部署最重要的是先定好工作目录，再决定配置文件、数据库、上传目录和临时目录放在哪里。

正式上线时，不要直接把 `aster_drive` 暴露到公网。
systemd 只负责拉起进程；HTTPS、域名、HSTS、上传限制和 WebDAV / WOPI 透传都应该交给前面的反向代理。浏览器页面基线 `Content-Security-Policy` 由 AsterDrive 返回，代理层应保留该响应头，避免覆盖成不兼容的策略。

## 1. 准备运行目录

```bash
sudo useradd -r -s /usr/sbin/nologin asterdrive
sudo mkdir -p /var/lib/asterdrive
sudo chown -R asterdrive:asterdrive /var/lib/asterdrive
```

## 2. 放置可执行文件

把 `aster_drive` 可执行文件放到固定路径，例如：

```bash
sudo install -m 0755 ./aster_drive /usr/local/bin/aster_drive
```

## 3. 准备配置文件

把 `config.toml` 放进 `data/` 目录：

```bash
sudo mkdir -p /var/lib/asterdrive/data
sudo cp config.toml /var/lib/asterdrive/data/config.toml
sudo chown -R asterdrive:asterdrive /var/lib/asterdrive/data
```

如果继续使用默认相对路径，工作目录下通常会出现：

- `data/config.toml`
- `data/asterdrive.db`
- `data/uploads`
- `data/.tmp`
- `data/.uploads`

长期部署时，建议数据库路径、本地存储路径和临时目录都尽量使用绝对路径。

如果你准备长期运行，不要只考虑“怎么启动”，还要同时规划备份和恢复。
SQLite、本地存储目录、头像目录和你自定义的本地 `local` 存储根目录，都应该纳入同一套备份策略。见 [备份与恢复](/deployment/backup)。

## 4. 写入 Service 文件

创建 `/etc/systemd/system/asterdrive.service`：

```ini
[Unit]
Description=AsterDrive
After=network.target

[Service]
Type=simple
User=asterdrive
Group=asterdrive
WorkingDirectory=/var/lib/asterdrive
ExecStart=/usr/local/bin/aster_drive
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

如果你现在还是内网 HTTP 测试环境，可以先在 `config.toml` 里把 `auth.bootstrap_insecure_cookies` 设成 `true`。
它只会影响第一次初始化时 Cookie 的 HTTPS 要求。正式切到 HTTPS 后，把后台对应设置改回开启，再把这个静态引导项去掉。

## 5. 启动服务

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now asterdrive
sudo systemctl status asterdrive
```

## 6. 查看日志

```bash
journalctl -u asterdrive -f
```

## 7. 首次验收

- `/health` 返回 200
- `/health/ready` 返回 200
- 首页响应头里已经带上 AsterDrive 返回的页面基线 `Content-Security-Policy`
- 首次启动日志里已完成数据库更新和默认策略初始化
- 管理后台里能看到默认策略组
- 浏览器可以正常登录
- 如果启用了 WebDAV，实际挂载路径与 `[webdav].prefix` 一致
- 如果启用了外部 Office / WOPI 打开方式，至少用一个真实 Office 文件试开并保存一次
- 如果把数据库、上传目录或临时目录放到其他磁盘，确认路径和权限没有写错

## 8. 常见环境变量写法

### 把数据库放到其他位置

```ini
Environment=ASTER__DATABASE__URL=sqlite:///srv/asterdrive/asterdrive.db?mode=rwc
```

### 监听所有网卡

```ini
Environment=ASTER__SERVER__HOST=0.0.0.0
```

### 固定登录签名密钥

```ini
Environment=ASTER__AUTH__JWT_SECRET=replace-with-your-own-secret
```

### 把这台服务跑成从节点

```ini
Environment=ASTER__SERVER__START_MODE=follower
Environment=ASTER__SERVER__FOLLOWER__REMOTE_STORAGE_TARGET_LOCAL_ROOT=/srv/asterdrive/remote-storage-targets
```

如果还想让从节点启动时自动完成一次性 enroll，可以临时加上：

```ini
Environment=ASTER_BOOTSTRAP_REMOTE_MASTER_URL=https://drive.example.com
Environment=ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN=enr_replace_me
```

确认接入成功后，建议删掉这两个 `ASTER_BOOTSTRAP_REMOTE_*`，只保留长期运行需要的 `ASTER__...` 覆盖项。完整流程见 [远程节点](/guide/remote-nodes)。

## 9. HTTPS 与域名

systemd 只负责把服务拉起来。
如果你要提供 HTTPS、域名、WebDAV 客户端访问，或者外部 Office / WOPI 打开方式，还需要在前面加一层反向代理。代理层负责 TLS、HSTS、上传限制和长连接透传，同时保留 AsterDrive 返回的页面 CSP，见 [反向代理部署](/deployment/reverse-proxy)。
