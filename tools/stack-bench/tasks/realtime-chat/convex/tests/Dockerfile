# Verifier container (run separately from the environment, per TB3).
FROM node:20-bookworm

WORKDIR /tests
COPY . /tests/

# TODO(spike): pre-install + pre-build deps here so the verifier needs no
# network at grade time. The spike installs at run time for faster iteration.

CMD ["bash", "test.sh"]
