#!/bin/bash
# 1. Boot the Kali backend API silently in the background
kali-server-mcp --ip 127.0.0.1 --port 5000 > /dev/null 2>&1 &

# Give it 2 seconds to fully start
sleep 2

# 2. Boot the official STDIO bridge in the foreground
exec mcp-server --server http://127.0.0.1:5000
