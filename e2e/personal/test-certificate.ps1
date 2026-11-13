$ErrorActionPreference='Stop'
# No certificate store or trust settings are changed. This certificate is used
# only by the loopback test proxy and accepted only by its Playwright context.
$rsa=[Security.Cryptography.RSA]::Create(2048)
try {
    $request=[Security.Cryptography.X509Certificates.CertificateRequest]::new(
        'CN=127.0.0.1',$rsa,[Security.Cryptography.HashAlgorithmName]::SHA256,
        [Security.Cryptography.RSASignaturePadding]::Pkcs1)
    $names=[Security.Cryptography.X509Certificates.SubjectAlternativeNameBuilder]::new()
    $names.AddIpAddress([Net.IPAddress]::Loopback)
    $request.CertificateExtensions.Add($names.Build())
    $certificate=$request.CreateSelfSigned([DateTimeOffset]::UtcNow.AddDays(-1),[DateTimeOffset]::UtcNow.AddDays(1))
    try { [Convert]::ToBase64String($certificate.Export([Security.Cryptography.X509Certificates.X509ContentType]::Pfx,'synthetic-test-certificate')) }
    finally { $certificate.Dispose() }
} finally { $rsa.Dispose() }
