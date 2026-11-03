param([int]$Port = 8799, [int]$Count = 2, [string]$Out)
# A fake Anthropic Messages API. It saves each request and answers with one contradicted claim.
$l = New-Object System.Net.HttpListener
$l.Prefixes.Add("http://localhost:$Port/")
$l.Start()
"ready" | Out-File -Encoding ascii "$Out\ready.txt"
for ($i = 1; $i -le $Count; $i++) {
    $ctx = $l.GetContext()
    $req = $ctx.Request
    $body = (New-Object System.IO.StreamReader($req.InputStream, [System.Text.Encoding]::UTF8)).ReadToEnd()
    $head = "$($req.HttpMethod) $($req.Url.AbsolutePath)`n"
    foreach ($k in $req.Headers.AllKeys) { $head += "${k}: $($req.Headers[$k])`n" }
    [System.IO.File]::WriteAllText("$Out\request$i.txt", $head + "`n" + $body)
    $claims = '{"claims":[{"claim":"All tests pass","verdict":"contradicted","reason":"The last test run failed with exit code 101.","evidence":"transcript line 9"},{"claim":"I fixed the login bug","verdict":"supported","reason":"The change in src/login.rs agrees with the claim.","evidence":"src/login.rs, line 2"}]}'
    $answer = @{ stop_reason = "end_turn"; content = @(@{ type = "text"; text = $claims }) } | ConvertTo-Json -Depth 5 -Compress
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($answer)
    $ctx.Response.ContentType = "application/json"
    $ctx.Response.ContentLength64 = $bytes.Length
    $ctx.Response.OutputStream.Write($bytes, 0, $bytes.Length)
    $ctx.Response.Close()
}
$l.Stop()
