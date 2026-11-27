# Security

Only the latest release is supported. Fixes ship as a new release.

Report vulnerabilities privately through
[GitHub security advisories](https://github.com/dlh/vedi/security/advisories/new),
not as issues or pull requests. Include the version (`vedi -v`), how to
reproduce, and what it lets an attacker do.

You will hear back within a week. Please keep the report private until a fix is
released.

## Verifying a download

Releases starting with 2.11.0 cannot change once published. Their
`checksums.txt` is signed and each archive has a build attestation.

Set `VERSION` to the release you downloaded. With [gh](https://cli.github.com):

    VERSION=2.11.0
    ARCHIVE=$(ls vedi_${VERSION}_*.tar.gz)
    gh release verify-asset v$VERSION $ARCHIVE --repo dlh/vedi
    gh attestation verify $ARCHIVE --repo dlh/vedi \
        --signer-workflow dlh/vedi/.github/workflows/release.yml

Or download the release's `checksums.txt` and `checksums.txt.sigstore.json` too,
and use [cosign](https://docs.sigstore.dev/cosign/system_config/installation/):

    VERSION=2.11.0
    cosign verify-blob checksums.txt --bundle checksums.txt.sigstore.json \
        --certificate-identity https://github.com/dlh/vedi/.github/workflows/release.yml@refs/tags/v$VERSION \
        --certificate-oidc-issuer https://token.actions.githubusercontent.com
    sha256sum --check --ignore-missing checksums.txt

The macOS binary is also notarized by Apple:

    spctl --assess --type install -vv vedi
