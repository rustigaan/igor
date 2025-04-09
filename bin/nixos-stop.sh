#!/bin/bash

set -e

BIN="$(cd "$(dirname "$0")" ; pwd)"
PROJECT="$(dirname "${BIN}")"

source "${BIN}/lib-verbose.sh"

(
  cd "${PROJECT}/docker-compose/containerized-nix"
  docker compose stop
)
