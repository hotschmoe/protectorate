# Global ARGs for base image (must be before FROM that uses it)
ARG BASE_IMAGE=protectorate/base
ARG BASE_TAG=latest

# Stage 1: Build Go binary
FROM golang:1.24-alpine AS builder

WORKDIR /build
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -o envoy ./cmd/envoy

# Stage 2: Runtime on shared base
FROM ${BASE_IMAGE}:${BASE_TAG}

COPY --from=builder /build/envoy /usr/local/bin/envoy
COPY containers/envoy/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 7470

ENTRYPOINT ["/entrypoint.sh"]
