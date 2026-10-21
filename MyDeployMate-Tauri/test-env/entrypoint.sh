#!/bin/sh
set -e
# Host keys are generated per container so tests can exercise first-trust and key-change flows.
[ -f /etc/ssh/ssh_host_ed25519_key ] || ssh-keygen -q -t ed25519 -N '' -f /etc/ssh/ssh_host_ed25519_key
[ -f /etc/ssh/ssh_host_rsa_key ] || ssh-keygen -q -t rsa -b 3072 -N '' -f /etc/ssh/ssh_host_rsa_key

# Authorize every public key mounted under /keys for every test user.
if ls /keys/*.pub >/dev/null 2>&1; then
  for u in deploy nopass nosudo ttyuser; do
    install -d -m 700 -o "$u" -g "$u" "/home/$u/.ssh"
    cat /keys/*.pub > "/home/$u/.ssh/authorized_keys"
    chown "$u:$u" "/home/$u/.ssh/authorized_keys" && chmod 600 "/home/$u/.ssh/authorized_keys"
  done
fi

exec /usr/sbin/sshd -D -e -f "/etc/ssh/sshd_config.${SSHD_PROFILE:-modern}"
