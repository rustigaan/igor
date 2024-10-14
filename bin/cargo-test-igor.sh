#!/bin/bash

export RUST_LOG='info,igor=debug'

cargo --color always test -- --nocapture 2>&1 | tee ~/tmp/cargo-test-igor.log