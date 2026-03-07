use core::fmt::Write as _;
use embassy_net::Stack;
use embassy_net::tcp::TcpSocket;
use embassy_time::Duration;
use embedded_io_async::Write;

const INDEX_HTML: &str = r#"HTTP/1.1 200 OK
Content-Type: text/html
Connection: close

<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Pico 2W Web Shell</title>
<style>
    body { font-family: 'Inter', sans-serif; background-color: #0f172a; color: #f8fafc; margin: 0; padding: 20px; display: flex; flex-direction: column; align-items: center; }
    h1 { color: #38bdf8; margin-bottom: 5px; }
    h3 { color: #94a3b8; font-size: 14px; margin-top: 0; margin-bottom: 20px; }
    #terminal { width: 100%; max-width: 800px; height: 50vh; background: #1e293b; border: 1px solid #334155; border-radius: 8px; padding: 15px; overflow-y: auto; font-family: 'Courier New', monospace; white-space: pre-wrap; margin-bottom: 20px; box-shadow: 0 4px 6px rgba(0,0,0,0.3); }
    .input-container { width: 100%; max-width: 800px; display: flex; gap: 10px; }
    input[type="text"] { flex-grow: 1; padding: 12px 15px; border: none; border-radius: 6px; background: #1e293b; color: #f8fafc; font-family: 'Courier New', monospace; font-size: 16px; outline: none; border: 1px solid #334155; transition: border-color 0.3s; }
    input[type="text"]:focus { border-color: #38bdf8; }
    button { padding: 12px 25px; border: none; border-radius: 6px; background: #38bdf8; color: #0f172a; font-weight: bold; font-size: 16px; cursor: pointer; transition: background 0.3s, transform 0.1s; }
    button:hover { background: #7dd3fc; }
    button:active { transform: scale(0.98); }
    .cmd-line { color: #cbd5e1; }
    .out-line { color: #38bdf8; }
    .err-line { color: #f87171; }
</style>
</head>
<body>
    <h1>Pico 2W Web Shell</h1>
    <h3>Connected to SoftAP</h3>
    <div id="terminal">Welcome to Pico 2W Shell. Type 'help' to see available commands.<br><br></div>
    <div class="input-container">
        <input type="text" id="cmdInput" placeholder="Enter command here..." autocomplete="off" autofocus>
        <button onclick="sendCommand()">Execute</button>
    </div>

    <script>
        const terminal = document.getElementById('terminal');
        const input = document.getElementById('cmdInput');

        input.addEventListener('keypress', function (e) {
            if (e.key === 'Enter') sendCommand();
        });

        async function sendCommand() {
            const cmd = input.value.trim();
            if (!cmd) return;
            
            terminal.innerHTML += `<span class="cmd-line">> ${cmd}</span>\n`;
            input.value = '';
            terminal.scrollTop = terminal.scrollHeight;

            try {
                const response = await fetch('/cmd', {
                    method: 'POST',
                    body: cmd
                });
                const result = await response.text();
                const escapedResult = result.replace(/</g, "&lt;").replace(/>/g, "&gt;");
                terminal.innerHTML += `<span class="out-line">${escapedResult}</span>\n`;
            } catch (err) {
                terminal.innerHTML += `<span class="err-line">Error: ${err.message}</span>\n`;
            }
            terminal.scrollTop = terminal.scrollHeight;
        }
    </script>
</body>
</html>
"#;

const INDEX_HTML_SETUP: &str = r#"HTTP/1.1 200 OK
Content-Type: text/html
Connection: close

<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Pico 2W WiFi Setup</title>
<style>
    body { font-family: 'Inter', sans-serif; background-color: #0f172a; color: #f8fafc; margin: 0; padding: 20px; display: flex; flex-direction: column; align-items: center; }
    h1 { color: #38bdf8; margin-bottom: 5px; }
    h3 { color: #94a3b8; font-size: 14px; margin-top: 0; margin-bottom: 20px; }
    .card { background: #1e293b; border: 1px solid #334155; border-radius: 8px; padding: 25px; width: 100%; max-width: 400px; box-shadow: 0 4px 6px rgba(0,0,0,0.3); display: flex; flex-direction: column; gap: 15px; }
    label { font-size: 14px; color: #cbd5e1; }
    input[type="text"], input[type="password"] { width: calc(100% - 30px); padding: 12px 15px; border: none; border-radius: 6px; background: #0f172a; color: #f8fafc; font-size: 16px; outline: none; border: 1px solid #334155; transition: border-color 0.3s; }
    input:focus { border-color: #38bdf8; }
    button { padding: 12px; border: none; border-radius: 6px; background: #38bdf8; color: #0f172a; font-weight: bold; font-size: 16px; cursor: pointer; transition: background 0.3s, transform 0.1s; width: 100%; margin-top: 10px; }
    button:hover { background: #7dd3fc; }
    button:active { transform: scale(0.98); }
    .status { text-align: center; color: #f87171; font-size: 14px; margin-top: 10px; }
    .success { color: #4ade80; }
</style>
</head>
<body>
    <h1>Pico 2W Setup</h1>
    <h3>Configure Wi-Fi Network</h3>
    <div class="card">
        <div>
            <label for="ssid">Network Name (SSID)</label><br>
            <input type="text" id="ssid" placeholder="ex) My Home WiFi">
        </div>
        <div>
            <label for="pass">Password</label><br>
            <input type="password" id="pass" placeholder="Network password">
        </div>
        <button onclick="saveConfig()">Save & Reboot</button>
        <div id="status" class="status"></div>
    </div>

    <script>
        async function saveConfig() {
            const ssid = document.getElementById('ssid').value.trim();
            const pass = document.getElementById('pass').value;
            const statusLabel = document.getElementById('status');
            
            if (!ssid) {
                statusLabel.className = 'status';
                statusLabel.innerText = 'SSID cannot be empty!';
                return;
            }

            statusLabel.className = 'status success';
            statusLabel.innerText = 'Saving...';
            
            try {
                const response = await fetch('/connect', {
                    method: 'POST',
                    body: ssid + '\n' + pass
                });
                
                if (response.ok) {
                    statusLabel.innerText = 'Saved! Device is rebooting.';
                } else {
                    statusLabel.className = 'status';
                    statusLabel.innerText = 'Failed to save configuration.';
                }
            } catch (err) {
                statusLabel.className = 'status';
                statusLabel.innerText = 'Error: ' + err.message;
            }
        }
    </script>
</body>
</html>
"#;

#[embassy_executor::task]
pub async fn http_server_task(stack: Stack<'static>, is_sta: bool) {
    let mut rx_buffer = [0; 2048];
    let mut tx_buffer = [0; 4096]; // Larger TX buffer for HTML response

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

        defmt::info!("HTTP Server listening on TCP:80...");
        if let Err(e) = socket.accept(80).await {
            defmt::warn!("HTTP accept error: {:?}", e);
            continue;
        }

        defmt::info!("HTTP Client connected!");

        let mut buf = [0; 2048];
        let mut n = 0;

        loop {
            match socket.read(&mut buf[n..]).await {
                Ok(0) => {
                    break;
                }
                Ok(len) => {
                    n += len;
                    if let Ok(req) = core::str::from_utf8(&buf[..n]) {
                        if req.starts_with("GET ") && req.contains("\r\n\r\n") {
                            break;
                        } else if req.starts_with("POST ") {
                            let cl_idx = req
                                .find("Content-Length:")
                                .or_else(|| req.find("content-length:"))
                                .or_else(|| req.find("Content-length:"));

                            if let Some(idx) = cl_idx {
                                let mut cl = 0;
                                let after_cl = &req[idx + 15..];
                                if let Some(rn_idx) = after_cl.find("\r\n") {
                                    cl = after_cl[..rn_idx].trim().parse::<usize>().unwrap_or(0);
                                }
                                if let Some(body_idx) = req.find("\r\n\r\n") {
                                    let body_len = req.len() - (body_idx + 4);
                                    if body_len >= cl {
                                        break;
                                    }
                                }
                            } else if req.contains("\r\n\r\n") {
                                break;
                            }
                        } else if req.contains("\r\n\r\n") {
                            break; // other methods
                        }
                    }
                    if n == buf.len() {
                        defmt::warn!("HTTP buffer full");
                        break;
                    }
                }
                Err(e) => {
                    defmt::warn!("HTTP Read Error: {:?}", e);
                    break;
                }
            }
        }

        if n > 0 {
            if let Ok(req) = core::str::from_utf8(&buf[..n]) {
                if req.starts_with("GET / ") {
                    // Serve single page app
                    let html = if is_sta { INDEX_HTML } else { INDEX_HTML_SETUP };
                    if let Err(e) = socket.write_all(html.as_bytes()).await {
                        defmt::warn!("HTTP Write Error: {:?}", e);
                    }
                } else if req.starts_with("GET /scan ") && !is_sta {
                    defmt::info!("HTTP GET /scan received");
                    // Clear stale responses
                    while let Ok(_) = crate::WEB_RESP_CHANNEL.try_receive() {}

                    let mut cmd_str = heapless::String::<256>::new();
                    let _ = cmd_str.push_str("sys_scan");
                    let web_cmd = crate::WebCommand { cmd: cmd_str };
                    let _ = crate::WEB_CMD_CHANNEL.send(web_cmd).await;

                    let response_str = crate::WEB_RESP_CHANNEL.receive().await;

                    let mut header = heapless::String::<128>::new();
                    core::write!(
                        &mut header,
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
                        response_str.len()
                    ).ok();

                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(response_str.as_bytes()).await;
                } else if req.starts_with("POST /connect ") && !is_sta {
                    defmt::info!("HTTP POST /connect received");
                    let parts: heapless::Vec<&str, 2> = req.splitn(2, "\r\n\r\n").collect();
                    if parts.len() == 2 {
                        let body = parts[1].trim_matches(char::from(0)).trim();
                        let cred_parts: heapless::Vec<&str, 2> = body.split('\n').collect();
                        if cred_parts.len() == 2 {
                            let ssid = cred_parts[0].trim();
                            let pass = cred_parts[1].trim();

                            defmt::info!("Saving new Wi-Fi credentials for: '{}'", ssid);
                            if let Ok(_) = crate::logger::write_wifi_conf(ssid, pass).await {
                                let _ = socket
                                    .write_all(
                                        b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nSuccess",
                                    )
                                    .await;
                                let _ = socket.flush().await;
                                socket.close();

                                defmt::info!("Rebooting to apply new configuration...");
                                embassy_time::Timer::after(Duration::from_millis(500)).await;
                                cortex_m::peripheral::SCB::sys_reset();
                            } else {
                                let _ = socket.write_all(b"HTTP/1.1 500 Internal Server Error\r\nConnection: close\r\n\r\nFS Error").await;
                            }
                        } else {
                            let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nInvalid Body").await;
                        }
                    }
                } else if req.starts_with("POST /cmd ") && is_sta {
                    // Extract body
                    defmt::info!("HTTP POST received");
                    let parts: heapless::Vec<&str, 2> = req.splitn(2, "\r\n\r\n").collect();
                    if parts.len() == 2 {
                        let body = parts[1].trim_matches(char::from(0)).trim();
                        defmt::info!("Parsed Body: '{}'", body);

                        // Handle empty body without blocking
                        if body.is_empty() {
                            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n[Empty command ignored]\r\n").await;
                            let _ = socket.flush().await;
                            socket.close();
                            continue;
                        }

                        let mut cmd_str = heapless::String::<256>::new();
                        let _ = cmd_str.push_str(body);

                        // Clear stale responses
                        while let Ok(_) = crate::WEB_RESP_CHANNEL.try_receive() {}

                        let web_cmd = crate::WebCommand { cmd: cmd_str };
                        let _ = crate::WEB_CMD_CHANNEL.send(web_cmd).await;

                        // Wait for response
                        let response_str = crate::WEB_RESP_CHANNEL.receive().await;
                        defmt::info!("Sending Response: '{}'", response_str.as_str());

                        // Form HTTP OK with strict headers
                        let mut header = heapless::String::<128>::new();
                        core::write!(
                            &mut header,
                            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
                            response_str.len()
                        ).ok();

                        let _ = socket.write_all(header.as_bytes()).await;
                        let _ = socket.write_all(response_str.as_bytes()).await;
                    } else {
                        defmt::warn!("POST bad format parts: {}", parts.len());
                        let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nMissing Body").await;
                    }
                } else {
                    // Not Found
                    let _ = socket
                        .write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n")
                        .await;
                }
            }
        }

        let _ = socket.flush().await;
        socket.close();
    }
}
