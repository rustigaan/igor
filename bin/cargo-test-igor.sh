#!/bin/bash

export RUST_LOG='info,igor=debug'

COLOR='always'
if [[ ".$1" = '.--color' ]]
then
  COLOR="$2"
  shift 2
fi

cargo --color "${COLOR}" test -- --nocapture "$@" 2>&1 | tee ~/tmp/cargo-test-igor.log
