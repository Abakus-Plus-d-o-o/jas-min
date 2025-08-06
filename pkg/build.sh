#!/bin/bash -eu
#
#

if [ "$#" -lt 1 ] || [ "$#" -gt 3 ]
then
    echo "Usage: $0 <deb|rpm|...> <distro> <debug|release>"
    exit 1
fi

CFG_TYPE="$1"
CFG_DIST="${2:-debian:latest}"
CFG_RELEASE="${3:-release}"

case $CFG_DIST in
    debian*|ubuntu*)
        cat <<EOF > Dockerfile
FROM $CFG_DIST

COPY ./jas-min-ctl.sh /opt/jas-min-ctl.sh

SHELL ["/bin/bash", "-c"]
RUN apt update -y && apt full-upgrade -y && apt install -y git curl build-essential pkg-config libssl-dev chromium-driver && apt clean && \
    chmod a+x /opt/jas-min-ctl.sh && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
EOF
        ;;

    *)
        echo "Currently unsupported build distribution [$CFG_DIST]."
        exit 1
esac

mkdir -p ./tmp/
case "$CFG_TYPE" in
    deb)
        docker build -t jasmin-build .
        docker run -v "$(pwd)/tmp:/opt/jas-min-pkg/" -it jasmin-build /opt/jas-min-ctl.sh "$CFG_TYPE" "$CFG_RELEASE"
        ;;
esac


