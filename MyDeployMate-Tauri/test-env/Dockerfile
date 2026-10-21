# Linux SSH target for MyDeployMate integration tests. Test use only: weak passwords, never expose publicly.
FROM ubuntu:24.04

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-server sudo procps curl ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /run/sshd

# deploy:   sudo needs a password (tests sudo -S)
# nopass:   NOPASSWD for everything (tests sudo -n success)
# nosudo:   no sudo rights at all (tests sudo -n refusal)
# ttyuser:  sudo needs a password and requiretty (tests the "requires a terminal" error)
RUN for u in deploy nopass nosudo ttyuser; do useradd -m -s /bin/bash "$u" && echo "$u:$u-pass" | chpasswd; done \
 && echo 'deploy  ALL=(ALL) ALL'                   >  /etc/sudoers.d/mdm \
 && echo 'nopass  ALL=(ALL) NOPASSWD: ALL'         >> /etc/sudoers.d/mdm \
 && echo 'Defaults:ttyuser requiretty'             >> /etc/sudoers.d/mdm \
 && echo 'ttyuser ALL=(ALL) ALL'                   >> /etc/sudoers.d/mdm \
 && chmod 440 /etc/sudoers.d/mdm && visudo -cf /etc/sudoers.d/mdm \
 && mkdir -p /opt/protected && chown root:root /opt/protected && chmod 755 /opt/protected

COPY sshd_config.modern /etc/ssh/sshd_config.modern
COPY sshd_config.legacy /etc/ssh/sshd_config.legacy
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 22
ENTRYPOINT ["/entrypoint.sh"]
