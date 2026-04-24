#!/bin/bash

# Generate a random 12-character alphanumeric string
cat /dev/urandom | LC_ALL=C tr -dc 'a-zA-Z0-9' | fold -w 12 | head -n 1
