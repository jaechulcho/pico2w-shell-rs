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

#[embassy_executor::task]
pub async fn http_server_task(stack: Stack<'static>) {
    let mut rx_buffer = [0; 1024];
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
                    if let Err(e) = socket.write_all(INDEX_HTML.as_bytes()).await {
                        defmt::warn!("HTTP Write Error: {:?}", e);
                    }
                } else if req.starts_with("POST /cmd ") {
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
