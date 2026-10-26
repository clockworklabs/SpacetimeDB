FROM ghcr.io/dena/unity-meta-check/unity-meta-check-gh-action:3.3.0

# Remove both root paths, including the AAA cross-sign, from this container.
RUN sed -i '\|mozilla/emSign_ECC_Root_CA_-_G3.crt|d; \|mozilla/Comodo_AAA_Services_root.crt|d' /etc/ca-certificates.conf \
    && rm -f /usr/share/ca-certificates/mozilla/emSign_ECC_Root_CA_-_G3.crt \
             /usr/share/ca-certificates/mozilla/Comodo_AAA_Services_root.crt \
    && update-ca-certificates --fresh
