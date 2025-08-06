#!/bin/bash -eu
#
#

PKG_NAME='jas-min'
PKG_ARCH='amd64'
PKG_MAIN='"Urh Srecnik" <urh.srecnik@abakus.si>'
PKG_COPY='"Kamil Stawiarski" and "Radoslaw Kut"'
PKG_DESC='JSON AWR/STATSPACK Mining Tool.'

PKG_VERS='-'
PKG_FULL_NAME='-'


function proc_build() {
    mkdir -p /opt/jas-min-src
    pushd /opt/jas-min-src

    git clone https://github.com/ora600pl/jas-min.git .

    set +u
    set +e
    #source ~/.bashrc
    source ~/.cargo/env
    set -u
    set -e

    export WEBDRIVER_PATH=/usr/bin/chromedriver 
    if [ "$1" == 'release' ]
    then
        cargo build --release
    else
        cargo build
    fi

    popd
}

function proc_deb() {
    echo ' '
    echo 'Building .deb'
    echo '-------------'
    echo ' '    

    l_cf="/opt/jas-min-pkg/${PKG_FULL_NAME}/DEBIAN/control"
    l_cp="/opt/jas-min-pkg/${PKG_FULL_NAME}/DEBIAN/copyright"
    mkdir -p "$(dirname "$l_cf")"

    > "$l_cf"
    echo "Package: $PKG_NAME"      >> "$l_cf"
    echo "Version: $PKG_VERS"      >> "$l_cf"
    echo "Architecture: $PKG_ARCH" >> "$l_cf"
    echo "Maintainer: $PKG_MAIN"   >> "$l_cf"
    echo "Depends: libssl-dev, chromium-driver" >> "$l_cf"
    echo "Description: $PKG_DESC"  >> "$l_cf"
    echo ' n/a'                    >> "$l_cf" # extended description
    
    > "$l_cp"
    echo "Files: *"                 >> "$l_cp"
    echo "Copyright: $PKG_COPY"     >> "$l_cp"
    echo "License: MIT"             >> "$l_cp"

    dpkg-deb -b "/opt/jas-min-pkg/$PKG_FULL_NAME"
    
#   rm -r "$(dirname "$l_cf")"
}

G_TYPE="$1"
G_RELEASE="$2"

proc_build "$G_RELEASE"
PKG_VERS="$(cat /opt/jas-min-src/Cargo.toml  | grep 'version' | head -n 1 | cut -d '=' -f2 | sed 's/\"//g' |  awk '{$1=$1; print}')"
PKG_FULL_NAME="${PKG_NAME}-${PKG_VERS}"

mkdir -p "/opt/jas-min-pkg/${PKG_FULL_NAME}/opt/jas-min/"
 
if [ -d /opt/jas-min-src/target/debug ]
then
    cp -v  /opt/jas-min-src/target/debug/jas-min "/opt/jas-min-pkg/${PKG_FULL_NAME}/opt/jas-min/"
elif [ -d /opt/jas-min-src/target/release ]
then
     cp -v  /opt/jas-min-src/target/release/jas-min "/opt/jas-min-pkg/${PKG_FULL_NAME}/opt/jas-min/"
else
    echo "jas-min is not yet built."
    exit 1
fi
case "$G_TYPE" in
    deb)
        proc_deb
        ;;

    *)
        echo 'Invalid build option'
esac


