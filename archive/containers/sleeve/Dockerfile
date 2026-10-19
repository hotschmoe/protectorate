ARG BASE_IMAGE=protectorate/base
ARG BASE_TAG=latest
FROM ${BASE_IMAGE}:${BASE_TAG}

COPY bin/sidecar /usr/local/bin/sidecar
COPY containers/sleeve/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh /usr/local/bin/sidecar

ENTRYPOINT ["/entrypoint.sh"]
