#!/bin/bash

set -e

BIN="$(cd "$(dirname "$0")" ; pwd)"
PROJECT="$(dirname "${BIN}")"

source "${BIN}/lib-verbose.sh"

DETACH='true'
if [[ ".$1" = '.--attach' ]]
then
  DETACH='false'
fi

COMPOSE_ARGS=()
if "${DETACH}"
then
  COMPOSE_ARGS+=(--detach)
fi

VOLUME_NAME='nix-store'
VOLUME="$(docker volume ls --filter name="^${VOLUME_NAME}\$" --format '{{.Name}}')"
if [[ -z "${VOLUME}" ]]
then
  docker volume create "${VOLUME_NAME}"
  docker run --rm -v "${VOLUME_NAME}:/mnt/nix" nixpkgs/nix-flakes bash -c 'tar -C /nix -cf - . | tar -C /mnt/nix --mode ug+rw -xvf -'
fi

(
  cd "${PROJECT}/docker-compose/containerized-nix"
  docker compose up "${COMPOSE_ARGS[@]}"
)
