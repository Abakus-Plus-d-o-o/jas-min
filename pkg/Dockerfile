FROM debian:latest

COPY ./jas-min-ctl.sh /opt/jas-min-ctl.sh
COPY ./rest/Cargo.toml /opt/jas-min-rest-src/Cargo.toml
COPY ./rest/src/main.rs /opt/jas-min-rest-src/src/main.rs

SHELL ["/bin/bash", "-c"]
RUN apt update -y && apt full-upgrade -y && apt install -y git curl build-essential pkg-config libssl-dev chromium-driver && apt clean &&     chmod a+x /opt/jas-min-ctl.sh &&     curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
