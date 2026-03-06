
FROM kalilinux/kali-rolling
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y \
    mcp-kali-server \
    nmap \
    curl \
    gobuster \
    nikto \
    dirb \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
ENTRYPOINT ["kali-server-mcp"]
