---
title: Self-hosting
slug: /deploying/spacetimedb-standalone
---

# Self-hosting SpacetimeDB

This tutorial will guide you through setting up SpacetimeDB on an Ubuntu 24.04 server, securing it with HTTPS using Nginx and Let's Encrypt, and configuring a systemd service to keep it running.

## Prerequisites

- A fresh Ubuntu 24.04 server (VM or cloud instance of your choice)
- A domain name (e.g., `example.com`)
- `sudo` privileges on the server

## Step 1: Create a Dedicated User for SpacetimeDB

For security purposes, create a dedicated `spacetimedb` user to run SpacetimeDB:

```sh
sudo mkdir /stdb
sudo useradd --system spacetimedb
sudo chown -R spacetimedb:spacetimedb /stdb
```

Install SpacetimeDB as the new user:

```sh
sudo -u spacetimedb bash -c 'curl -sSf https://install.spacetimedb.com | sh -s -- --root-dir /stdb --yes'
```

## Step 2: Create a Systemd Service for SpacetimeDB

To ensure SpacetimeDB runs on startup, create a systemd service file:

```sh
sudo nano /etc/systemd/system/spacetimedb.service
```

Add the following content:

```systemd
[Unit]
Description=SpacetimeDB Server
After=network.target

[Service]
ExecStart=/stdb/spacetime --root-dir=/stdb start --listen-addr='127.0.0.1:3000'
Restart=always
User=spacetimedb
WorkingDirectory=/stdb

[Install]
WantedBy=multi-user.target
```

Enable and start the service:

```sh
sudo systemctl enable spacetimedb
sudo systemctl start spacetimedb
```

Check the status:

```sh
sudo systemctl status spacetimedb
```

## Step 3: Install and Configure Nginx

### Install Nginx

```sh
sudo apt update
sudo apt install nginx -y
```

### Configure Nginx Reverse Proxy

Create a new Nginx configuration file:

```sh
sudo nano /etc/nginx/sites-available/spacetimedb
```

Add the following configuration, remember to change `example.com` to your own domain:

```nginx
server {
    listen 80;
    server_name example.com;

    #########################################
    # By default SpacetimeDB is completely open so that anyone can publish to it. If you want to block
    # users from creating new databases you should keep this section commented out. Otherwise, if you
    # want to open it up (probably for dev environments) then you can uncomment this section and then
    # also comment out the location / section below.
    #########################################
    # location / {
    #     proxy_pass http://localhost:3000;
    #     proxy_http_version 1.1;
    #     proxy_set_header Upgrade $http_upgrade;
    #     proxy_set_header Connection "Upgrade";
    #     proxy_set_header Host $host;
    # }

    # Anyone can subscribe to any database.
    # Note: This is the only section *required* for the websocket to function properly. Clients will
    # be able to create identities, call reducers, and subscribe to tables through this websocket.
    location ~ ^/v1/database/[^/]+/subscribe$ {
        proxy_pass http://localhost:3000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "Upgrade";
        proxy_set_header Host $host;
    }

    # Uncomment this section to allow all HTTP reducer calls
    # location ~ ^/v1/[^/]+/call/[^/]+$ {
    #     proxy_pass http://localhost:3000;
    #     proxy_http_version 1.1;
    #     proxy_set_header Upgrade $http_upgrade;
    #     proxy_set_header Connection "Upgrade";
    #     proxy_set_header Host $host;
    # }

    # Uncomment this section to allow all HTTP sql requests
    # location ~ ^/v1/[^/]+/sql$ {
    #     proxy_pass http://localhost:3000;
    #     proxy_http_version 1.1;
    #     proxy_set_header Upgrade $http_upgrade;
    #     proxy_set_header Connection "Upgrade";
    #     proxy_set_header Host $host;
    # }

    # NOTE: This is required for the typescript sdk to function, it is optional
    # for the rust and the C# SDKs.
    location /v1/identity {
        proxy_pass http://localhost:3000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "Upgrade";
        proxy_set_header Host $host;
    }

    # Block all other routes explicitly. Only localhost can use these routes. If you want to open your
    # server up so that anyone can publish to it you should comment this section out.
    location / {
        allow 127.0.0.1;
        deny all;
    }
}
```

This configuration by default blocks all connections other than `/v1/identity` and `/v1/database/<database-name>/subscribe` which only allows the most basic functionality. This will prevent all remote users from publishing to your SpacetimeDB instance.

Enable the configuration:

```sh
sudo ln -s /etc/nginx/sites-available/spacetimedb /etc/nginx/sites-enabled/
```

Restart Nginx:

```sh
sudo systemctl restart nginx
```

### Configure Firewall

Ensure your firewall allows HTTPS traffic:

```sh
sudo ufw allow 'Nginx Full'
sudo ufw reload
```

## Step 4: Secure with Let's Encrypt

### Install Certbot

```sh
sudo apt install certbot python3-certbot-nginx -y
```

### Obtain an SSL Certificate

Run this command to request a new SSL cert from Let's Encrypt. Remember to replace `example.com` with your own domain:

```sh
sudo certbot --nginx -d example.com
```

Certbot will automatically configure SSL for Nginx. Restart Nginx to apply changes:

```sh
sudo systemctl restart nginx
```

### Auto-Renew SSL Certificates

Certbot automatically installs a renewal timer. Verify that it is active:

```sh
sudo systemctl status certbot.timer
```

## Step 5: Verify Installation

On your local machine, add this new server to your CLI config. Make sure to replace `example.com` with your own domain:

```bash
spacetime server add self-hosted --url https://example.com
```

If you have uncommented the `/v1/publish` restriction in Step 3 then you won't be able to publish to this instance unless you copy your module to the host first and then publish. We recommend something like this:

```bash
spacetime build
scp target/wasm32-unknown-unknown/release/spacetime_module.wasm ubuntu@<host>:/home/ubuntu/
ssh ubuntu@<host> spacetime publish -s local --bin-path spacetime_module.wasm <database-name>
```

You could put the above commands into a shell script to make publishing to your server easier and faster. It's also possible to integrate a script like this into Github Actions to publish on some event (like a PR merging into master).

## Step 6: Updating SpacetimeDB Version

To update SpacetimeDB to the latest version, first stop the service:

```sh
sudo systemctl stop spacetimedb
```

Then upgrade SpacetimeDB:

```sh
sudo -u spacetimedb -i -- spacetime --root-dir=/stdb version upgrade
```

To install a specific version, use:

```sh
sudo -u spacetimedb -i -- spacetime --root-dir=/stdb install <version-number>
```

Finally, restart the service:

```sh
sudo systemctl start spacetimedb
```

## Step 7: Troubleshooting

### SpacetimeDB Service Fails to Start

Check the logs for errors:

```sh
sudo journalctl -u spacetimedb --no-pager | tail -20
```

Verify that the `spacetimedb` user has the correct permissions:

```sh
sudo ls -lah /stdb/spacetime
```

If needed, add the executable permission:

```sh
sudo chmod +x /stdb/spacetime
```

### Let's Encrypt Certificate Renewal Issues

Manually renew the certificate and check for errors:

```sh
sudo certbot renew --dry-run
```

### Nginx Fails to Start

Test the configuration:

```sh
sudo nginx -t
```

If errors are found, check the logs:

```sh
sudo journalctl -u nginx --no-pager | tail -20
```
