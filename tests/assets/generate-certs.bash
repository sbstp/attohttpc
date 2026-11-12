#!/bin/bash

# Regenerates the test certificates used by the integration tests.
#
# The generated PEM files are committed to the repository, so this script only
# needs to be run deliberately, by a maintainer, when the certificates need to
# change. Requires OpenSSL 3.4+ for the -not_before and -not_after options.
#
# Note that expired-cert.pem is intentionally expired; do not "fix" it.
set -Eeuxo pipefail

cd "$(dirname "$0")"

# Server certificate presented by the hello world TLS server and the TLS proxy.
# Self-signed for CN=localhost, like self-signed.badssl.com.
#
# Note: the validity must stay under 825 days because macOS refuses to evaluate
# TLS server certificates with a longer validity period
# (errSSLValidityPeriodExceedsMaximum), even when the certificate would be
# accepted through `danger_accept_invalid_certs` or `add_root_certificate`.
# The serverAuth extended key usage is likewise required by macOS.
openssl req -x509 -newkey rsa:2048 -nodes -days 800 \
    -subj '/CN=localhost' \
    -addext 'subjectAltName=DNS:localhost' \
    -addext 'extendedKeyUsage=serverAuth' \
    -keyout key.pem -out cert.pem

# Test certificate authority used to sign the certificates below. Tests add it
# to the client's trust store using `add_root_certificate`.
openssl req -x509 -newkey rsa:2048 -nodes -days 36500 \
    -subj '/CN=attohttpc test CA' \
    -keyout ca-key.pem -out ca.pem

# Certificate for a different host, signed by the test CA. A client connecting
# to `localhost` receives a certificate with a valid chain that simply does not
# match the requested hostname, like wrong.host.badssl.com.
openssl req -new -newkey rsa:2048 -nodes \
    -subj '/CN=wronghost.example.com' \
    -addext 'subjectAltName=DNS:wronghost.example.com' \
    -addext 'extendedKeyUsage=serverAuth' \
    -keyout wrong-host-key.pem -out wrong-host.csr
openssl x509 -req -in wrong-host.csr -CA ca.pem -CAkey ca-key.pem -CAcreateserial \
    -copy_extensions copy -days 800 -out wrong-host-cert.pem

# Certificate for localhost with a valid chain that expired years ago, like
# expired.badssl.com.
openssl req -new -newkey rsa:2048 -nodes \
    -subj '/CN=localhost' \
    -addext 'subjectAltName=DNS:localhost' \
    -addext 'extendedKeyUsage=serverAuth' \
    -keyout expired-key.pem -out expired.csr
openssl x509 -req -in expired.csr -CA ca.pem -CAkey ca-key.pem -CAcreateserial \
    -copy_extensions copy -not_before 20190101000000Z -not_after 20200101000000Z -out expired-cert.pem

rm -f wrong-host.csr expired.csr ca.srl
