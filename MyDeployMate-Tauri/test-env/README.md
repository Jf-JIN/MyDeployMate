# 测试用 Linux 目标

用于集成测试和技术验证的 OpenSSH 服务器，**只在本机使用**（端口只绑定 127.0.0.1，密码很弱）。

```sh
./gen-keys.sh            # 生成测试用客户端私钥到 keys/（不入库）
docker compose up -d --build
```

运行 SSH 连接层集成测试（`crates/exec/tests/ssh_test_env.rs`）：`./run-ssh-tests.sh`。脚本会生成密钥、启动容器、启动临时 ssh-agent 并设置测试所需的环境变量；需要 docker、ssh-keygen、ssh-agent 和 puttygen。未设置 `MDM_TEST_ENV_KEYS` 时这些测试直接跳过，所以普通的 `cargo test` 不受影响。

| 服务 | 地址 | 说明 |
|---|---|---|
| modern | 127.0.0.1:2222 | 现代算法（ed25519 / rsa-sha2） |
| legacy | 127.0.0.1:2223 | 只提供 ssh-rsa（SHA-1），用于测试“允许旧算法”开关 |

| 用户 | 密码 | sudo 规则 | 用途 |
|---|---|---|---|
| deploy | deploy-pass | 需要密码 | `sudo -S` 运行时密码 |
| nopass | nopass-pass | NOPASSWD: ALL | `sudo -n` 免交互成功 |
| nosudo | nosudo-pass | 无 | `sudo -n` 被拒 |
| ttyuser | ttyuser-pass | 需要密码 + requiretty | “sudo 要求终端”错误 |

`keys/` 下所有 `.pub` 都会写入四个用户的 `authorized_keys`。`/opt/protected` 只有 root 可写，用于测试“权限不足时上传失败、不自动 sudo”。每个容器首次启动时生成自己的主机密钥；删除容器重建即可模拟“主机密钥变化”。
