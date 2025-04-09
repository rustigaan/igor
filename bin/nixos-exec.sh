#!/bin/bash

set -e

BIN="$(cd "$(dirname "$0")" ; pwd)"

source "${BIN}/lib-verbose.sh"

TTY='false'
if [[ -t 0 && -t 1 ]]
then
  TTY='true'
fi

if [[ ".$1" = '.--tty' ]]
then
  if [[ ".$2" = '.true' ]]
  then
    TTY='true'
  else
    TTY='false'
  fi
  shift 2
fi

if [[ ".$1" = '.--no-stdin' ]]
then
  shift
  exec < /dev/null
fi

if [[ ".$1" = '.--' ]]
then
  shift
fi

DOCKER_ARGS=()
if "${TTY}"
then
  DOCKER_ARGS+=(-t)
fi

COMMAND=(docker exec -i "${DOCKER_ARGS[@]}" nix "$@")
log "Command: [${COMMAND[*]}]"
"${COMMAND[@]}"
