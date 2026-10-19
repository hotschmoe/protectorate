FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    git \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# dtach: lightweight session manager (scroll works, unlike abduco)
ARG DTACH_VERSION=0.9
RUN curl -fsSL "https://github.com/crigler/dtach/archive/refs/tags/v${DTACH_VERSION}.tar.gz" \
    -o /tmp/dtach.tar.gz \
    && tar -xzf /tmp/dtach.tar.gz -C /tmp \
    && cd /tmp/dtach-${DTACH_VERSION} \
    && ./configure \
    && make \
    && mv dtach /usr/local/bin/ \
    && chmod +x /usr/local/bin/dtach \
    && rm -rf /tmp/dtach*

ARG CSTACK_VERSION=1.0.1
RUN curl -fsSL "https://github.com/Hotschmoe/cstack/releases/download/v${CSTACK_VERSION}/cstack-v${CSTACK_VERSION}-linux_amd64.tar.gz" \
    -o /tmp/cstack.tar.gz \
    && tar -xzf /tmp/cstack.tar.gz -C /tmp \
    && mv /tmp/cstack /usr/local/bin/cs \
    && chmod +x /usr/local/bin/cs \
    && rm /tmp/cstack.tar.gz

RUN useradd -m -s /bin/bash -u 1000 agent \
    && mkdir -p /home/agent/.claude /home/agent/.dtach /home/agent/.creds /home/agent/.config \
    && chown -R agent:agent /home/agent

USER agent
RUN curl -fsSL https://claude.ai/install.sh | bash \
    && echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
USER root

WORKDIR /home/agent
