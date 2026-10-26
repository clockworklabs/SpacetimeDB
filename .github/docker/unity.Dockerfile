ARG BASE_IMAGE
FROM ${BASE_IMAGE}

# Remove both root paths, including the AAA cross-sign, from this container.
RUN sed -i '\|mozilla/emSign_ECC_Root_CA_-_G3.crt|d; \|mozilla/Comodo_AAA_Services_root.crt|d' /etc/ca-certificates.conf \
    && rm -f /usr/share/ca-certificates/mozilla/emSign_ECC_Root_CA_-_G3.crt \
             /usr/share/ca-certificates/mozilla/Comodo_AAA_Services_root.crt \
    && update-ca-certificates --fresh

# Unity Package Manager has its own Node runtime; use the corrected OS bundle.
ENV NODE_OPTIONS=--use-openssl-ca \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
    UPM_GLOBAL_CONFIG_FILE=/etc/upmconfig.toml
RUN printf 'caFile = "/etc/ssl/certs/ca-certificates.crt"\n' > /etc/upmconfig.toml
