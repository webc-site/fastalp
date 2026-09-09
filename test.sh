#!/usr/bin/env bash

set -e
DIR=$(realpath $0) && DIR=${DIR%/*}
cd $DIR
. sh/env.sh
set -x
exec cargo nextest run --all-features "$@"
