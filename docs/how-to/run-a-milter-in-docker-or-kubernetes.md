# Run a milter in Docker or Kubernetes

This guide follows [configuration.md](../reference/configuration.md#environment-variables) and [tcp-server.md](../reference/tcp-server.md).

This guide shows how to package a rust-milter milter as a container image, configure it with environment variables, and run it next to Postfix in Docker or Kubernetes.

## Read the configuration from the environment

Build the server configuration from environment variables with the prefix `RUST_MILTER_`:

<!-- doctest-setup
use rust_milter::{Handler, HandlerContext};

struct MyHandler;

impl MyHandler {
    fn new(_ctx: &HandlerContext<()>) -> Self {
        MyHandler
    }
}

impl Handler for MyHandler {
    type Session = ();
    type Message = ();
}
-->
```rust
use rust_milter::{Server, ServerConfig};

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Logs go to standard output, where Docker and Kubernetes collect them.
    tracing_subscriber::fmt::init();

    let config = ServerConfig::from_env("RUST_MILTER_")?;

    let server = Server::builder()
        .config(config)
        .handler(MyHandler::new)
        .build()?;
    server.run().await?;
    Ok(())
}
```

To combine a configuration file with environment overrides, read the file first and call `config.apply_env("RUST_MILTER_")` on the result. The variable names are listed in [configuration.md](../reference/configuration.md#variable-names).

## Build the image

Build the binary in a Rust image and copy it into a small runtime image that runs as an unprivileged user:

```dockerfile
FROM rust:1.89 AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/my-milter /my-milter
EXPOSE 8891
ENTRYPOINT ["/my-milter"]
```

The milter becomes process 1 in the container and receives `SIGTERM` directly, which starts its graceful shutdown.

## Run it with Docker

Inside a container, the MTA connects from outside, so bind to all interfaces of the container and limit access on the host side:

```sh
docker run --rm --name milter \
  -p 127.0.0.1:8891:8891 \
  -e RUST_MILTER_BIND_ADDRESSES=0.0.0.0:8891 \
  -e RUST_MILTER_MAX_CONNECTIONS=2000 \
  --stop-timeout 45 \
  my-milter:latest
```

`docker stop` waits 10 seconds by default before it kills the process. rust-milter's default shutdown takes up to 35 seconds (`shutdown.grace_period` 5 s plus `shutdown.drain_timeout` 30 s), so `--stop-timeout 45` gives it room to drain open sessions.

With Docker Compose, put Postfix and the milter on one network:

```yaml
services:
  milter:
    image: my-milter:latest
    environment:
      RUST_MILTER_BIND_ADDRESSES: "0.0.0.0:8891"
      RUST_MILTER_MAX_CONNECTIONS: "2000"
    stop_grace_period: 45s
  postfix:
    image: my-postfix:latest
    depends_on:
      - milter
```

In Postfix's `main.cf`, point at the service name:

```text
smtpd_milters = inet:milter:8891
```

## Run it in Kubernetes

Create a Deployment. Each SMTP session uses its own milter connection, so a Service can spread sessions across replicas without session affinity:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: milter
  namespace: mail
spec:
  replicas: 2
  selector:
    matchLabels:
      app: milter
  template:
    metadata:
      labels:
        app: milter
    spec:
      terminationGracePeriodSeconds: 45
      containers:
        - name: milter
          image: registry.example.org/my-milter:1.0.0
          ports:
            - containerPort: 8891
          env:
            - name: RUST_MILTER_BIND_ADDRESSES
              value: "0.0.0.0:8891"
            - name: RUST_MILTER_MAX_CONNECTIONS
              value: "2000"
          readinessProbe:
            tcpSocket:
              port: 8891
            periodSeconds: 10
          livenessProbe:
            tcpSocket:
              port: 8891
            periodSeconds: 20
          lifecycle:
            preStop:
              sleep:
                seconds: 5
          securityContext:
            runAsNonRoot: true
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
```

Notes on these settings:
- The probes are TCP checks, because the milter protocol has no HTTP endpoint. Each probe opens and closes one connection, which rust-milter logs at DEBUG level and counts in its connection metrics.
- rust-milter stops accepting as soon as it receives `SIGTERM`, but Kubernetes removes the pod from the Service with a short delay. Kubernetes runs the `preStop` hook before it sends `SIGTERM`, so the sleep keeps the pod accepting during that delay. The `sleep` action needs Kubernetes 1.30 or later; on older clusters, use an `exec` hook in an image that has a `sleep` command.
- `terminationGracePeriodSeconds` must cover the pre-stop sleep plus rust-milter's shutdown: 5 + 5 + 30 = 40 seconds, so 45 leaves a margin. Kubernetes kills the container when this time runs out.

Expose the Deployment with a Service:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: milter
  namespace: mail
spec:
  selector:
    app: milter
  ports:
    - port: 8891
      targetPort: 8891
```

Allow connections only from the Postfix pods. The milter protocol has no authentication, so this is the only access control:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: milter
  namespace: mail
spec:
  podSelector:
    matchLabels:
      app: milter
  policyTypes:
    - Ingress
  ingress:
    - from:
        - podSelector:
            matchLabels:
              app: postfix
      ports:
        - port: 8891
          protocol: TCP
```

A NetworkPolicy takes effect only if the cluster's network plugin enforces it.

In Postfix's `main.cf`, point at the Service:

```text
smtpd_milters = inet:milter.mail.svc.cluster.local:8891
```

## Change the configuration

Environment variables cannot change inside a running process. To change one, edit the Deployment, or the Compose file, and restart:

```sh
kubectl -n mail rollout restart deployment/milter
```

The rollout replaces pods one at a time, and each old pod drains its sessions before it stops.

A configuration file mounted from a ConfigMap is updated in the pod after a delay, but nothing sends `SIGHUP`. Either restart the Deployment the same way, or send the signal yourself if the image has a `kill` command:

```sh
kubectl -n mail exec deploy/milter -- kill -HUP 1
```

The distroless image above has no `kill` command, so with it, restart instead.

## Check the file descriptor limit

Each MTA connection uses one file descriptor. Check the container's limit against `max_connections` plus the listeners plus 10, for example with Docker's `--ulimit nofile=` option. See [Deploy a milter in production](deploy-a-milter-in-production.md#raise-the-file-descriptor-limit).
