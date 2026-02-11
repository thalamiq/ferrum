#!/bin/bash
set -e

# Start worker in background
fhir-worker &
WORKER_PID=$!

# Start server in foreground
fhir-server &
SERVER_PID=$!

# Wait for either to exit
wait -n $WORKER_PID $SERVER_PID

# If one exits, kill the other
kill $WORKER_PID $SERVER_PID 2>/dev/null || true
