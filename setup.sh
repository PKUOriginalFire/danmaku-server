#!/usr/bin/env bash

if [ $# -ne 1 ]; then
    echo "Usage: $0 <account>"
    echo "  <account> - the account to use for the deployment"
    exit 1
fi

# get account from command line
ACCOUNT=$1

# Create .env if it doesn't exist
if [ ! -f .env ]; then
    cp .env.template .env
    sed -i '' "s/ACCOUNT=.*/ACCOUNT=$ACCOUNT/" .env
fi

# Create napcat/config/onebot11_$ACCOUNT.json
mkdir -p napcat/config
cp onebot.template.json napcat/config/onebot11_$ACCOUNT.json

echo "Setup complete. run \`docker compose up\` to start thr service."
