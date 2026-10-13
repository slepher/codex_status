"""Download AP01 0041 using a reusable local Xiaomi login; no device commands."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import queue
from pathlib import Path
import sys
import tempfile
import threading
import time
from urllib.parse import urlsplit

sys.dont_write_bytecode = True
PROJECT = Path(__file__).resolve().parents[2]
UPSTREAM = PROJECT.parent / "cuktech-ap01-firmware-tools"
sys.path.insert(0, str(UPSTREAM))

import requests
from features.official_firmware_source.login import XiaomiQrLogin
from features.official_firmware_source.xiaomi_cloud import XiaomiCloudClient

OUTPUT = PROJECT / "artifacts/ap01-research"
CREDENTIALS = OUTPUT / "xiaomi-account.dpapi"
MAX_ROM_BYTES = 16 * 1024 * 1024
VERSIONS = {"1.0.2_0041", "1.02_0041"}
FIRMWARE_HOSTS = {"bigota.d.miui.com", "iot-ota-cdn.io.mi.com"}


class DownloadRejected(RuntimeError):
    pass


def dpapi(payload: bytes, *, decrypt: bool = False) -> bytes:
    import ctypes
    from ctypes import wintypes
    class Blob(ctypes.Structure):
        _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_ubyte))]
    buffer = ctypes.create_string_buffer(payload)
    source = Blob(len(payload), ctypes.cast(buffer, ctypes.POINTER(ctypes.c_ubyte)))
    result = Blob()
    crypt = ctypes.WinDLL("crypt32", use_last_error=True)
    function = crypt.CryptUnprotectData if decrypt else crypt.CryptProtectData
    function.argtypes = [ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.c_void_p,
                         ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    function.restype = wintypes.BOOL
    if not function(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(result)):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        return ctypes.string_at(result.data, result.size)
    finally:
        free = ctypes.WinDLL("kernel32").LocalFree
        free.argtypes = [ctypes.c_void_p]
        free.restype = ctypes.c_void_p
        free(result.data)


def load_account() -> dict:
    payload = CREDENTIALS.read_bytes()
    if len(payload) > 16384:
        raise DownloadRejected("保存的登录文件长度异常")
    document = json.loads(dpapi(payload, decrypt=True))
    account = document.get("account", {})
    if document.get("format") != "AP01-XIAOMI-ACCOUNT-1" or any(
        not isinstance(account.get(key), str) or not account[key]
        for key in ("userId", "passToken", "deviceId")
    ):
        raise DownloadRejected("保存的登录文件格式不完整")
    return account


def save_account(account: dict) -> None:
    document = {"format": "AP01-XIAOMI-ACCOUNT-1", "account": account}
    payload = dpapi(json.dumps(document).encode("utf-8"))
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=OUTPUT, suffix=".login-part", delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, CREDENTIALS)
        if load_account() != account:
            raise DownloadRejected("登录凭据保存后读回失败")
    finally:
        if temporary:
            temporary.unlink(missing_ok=True)


def check_url(url: str, method: str, *, firmware: bool) -> None:
    parsed = urlsplit(url)
    host = parsed.hostname or ""
    if parsed.scheme != "https" or parsed.username or parsed.password or parsed.port not in (None, 443):
        raise DownloadRejected("拒绝非标准 HTTPS 地址")
    if firmware:
        allowed = host in FIRMWARE_HOSTS or any(
            host == domain or host.endswith("." + domain)
            for domain in ("mi-img.com", "fds.api.xiaomi.com", "fds.api.mi.com")
        )
        if not allowed or method != "GET":
            raise DownloadRejected("拒绝未列入白名单的固件下载目的地：" + host)
    elif host == "api.io.mi.com":
        if method != "POST" or parsed.path != "/app/home/latest_version" or parsed.query:
            raise DownloadRejected("仅允许型号固件查询，拒绝其他米家 API")
    elif method != "GET" or not (
        host in {"account.xiaomi.com", "sts.api.io.mi.com", "lp.account.xiaomi.com"}
        or host.endswith(".lp.account.xiaomi.com")
    ):
        raise DownloadRejected("拒绝未列入白名单的登录目的地：" + host)


class CheckedSession(requests.Session):
    def __init__(self, *, firmware: bool = False) -> None:
        super().__init__()
        self.firmware = firmware
        self.trust_env = False

    def send(self, request, **kwargs):
        # requests sends every redirected request through this method as well.
        check_url(request.url, request.method, firmware=self.firmware)
        if kwargs.get("verify") is False:
            raise DownloadRejected("拒绝关闭 TLS 证书校验")
        if self.firmware and any(key in request.headers for key in ("Cookie", "Authorization")):
            raise DownloadRejected("固件下载不得附带登录凭据")
        return super().send(request, **kwargs)


def check_version(info) -> None:
    if info.model != "njcuk.enstor.ap01" or info.version not in VERSIONS:
        raise DownloadRejected("小米云返回的型号或版本不是目标 AP01 0041，已停止")


def check_md5(actual: str, expected: str) -> None:
    if actual != expected:
        raise DownloadRejected("固件 MD5 与同次云端查询不符，已停止")


def save_rom(info) -> dict:
    check_version(info)
    check_url(info.url, "GET", firmware=True)
    target = OUTPUT / f"ap01-{info.version}.bin"
    report_path = target.with_suffix(".json")
    if target.exists() or report_path.exists():
        raise DownloadRejected("已有同名固件或记录，拒绝覆盖")
    md5, sha256 = hashlib.md5(), hashlib.sha256()
    size = 0
    temporary_path = None
    created_target = False
    try:
        with CheckedSession(firmware=True) as session:
            with session.get(info.url, timeout=(20, 30), stream=True) as response:
                response.raise_for_status()
                deadline = time.monotonic() + 180
                with tempfile.NamedTemporaryFile(dir=OUTPUT, suffix=".part", delete=False) as stream:
                    temporary_path = Path(stream.name)
                    for chunk in response.iter_content(65536):
                        if time.monotonic() > deadline:
                            raise DownloadRejected("固件下载超过时间预算")
                        size += len(chunk)
                        if size > MAX_ROM_BYTES:
                            raise DownloadRejected("固件超过 16 MiB 下载上限")
                        md5.update(chunk)
                        sha256.update(chunk)
                        stream.write(chunk)
                    stream.flush()
                    os.fsync(stream.fileno())
        if not size:
            raise DownloadRejected("固件为空")
        check_md5(md5.hexdigest(), info.md5)
        # Hard link commits the complete file without overwriting a concurrent file.
        os.link(temporary_path, target)
        created_target = True
        if hashlib.sha256(target.read_bytes()).hexdigest() != sha256.hexdigest():
            raise DownloadRejected("固件保存后读回校验失败")
        report = {
            "cloud": info.public_dict(), "path": str(target), "size": size,
            "md5": md5.hexdigest(), "sha256": sha256.hexdigest(),
            "device_update_sent": False, "credentials_saved": CREDENTIALS.is_file(),
        }
        with report_path.open("x", encoding="utf-8", newline="\n") as stream:
            # public_dict excludes download URLs and account credentials.
            json.dump(report, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
        return report
    except Exception:
        if created_target:
            target.unlink(missing_ok=True)
        raise
    finally:
        if temporary_path:
            temporary_path.unlink(missing_ok=True)


def download_with_retry(client) -> dict:
    """Keep account/session in the calling process during bounded operator retries."""
    for attempt in range(3):
        try:
            info = client.firmware_info()
            print("小米云查询结果：" + json.dumps(info.public_dict(), ensure_ascii=False), flush=True)
            print("固件下载域名：" + (urlsplit(info.url).hostname or ""), flush=True)
            return save_rom(info)
        except Exception as exc:
            reason = str(exc) if isinstance(exc, DownloadRejected) else type(exc).__name__
            print("下载未完成：" + reason, flush=True)
            if attempt == 2:
                raise DownloadRejected("三次尝试已结束") from None
            print("当前登录态仍在内存。5分钟内输入 retry 重试；其他输入取消。", flush=True)
            commands = queue.Queue(maxsize=1)
            def read_command():
                try:
                    command = input()
                except EOFError:
                    command = "cancel"
                commands.put(command.strip())
            threading.Thread(target=read_command, daemon=True).start()
            try:
                command = commands.get(timeout=300)
            except queue.Empty:
                raise DownloadRejected("等待重试超时，结束当前登录") from None
            if command != "retry":
                raise DownloadRejected("取消下载，结束当前登录")
    raise AssertionError("unreachable")


def self_test() -> None:
    from types import SimpleNamespace
    for url, method, firmware in (
        ("https://account.xiaomi.com/pass/serviceLogin", "GET", False),
        ("https://sts.api.io.mi.com/sts", "GET", False),
        ("https://c3.lp.account.xiaomi.com/longPolling", "GET", False),
        ("https://api.io.mi.com/app/home/latest_version", "POST", False),
        ("https://cdn.cnbj1.fds.api.mi-img.com/firmware.bin", "GET", True),
        ("https://cdn.cnbj1.fds.api.mi.com/firmware.bin", "GET", True),
        ("https://iot-ota-cdn.io.mi.com/firmware.bin", "GET", True),
    ):
        check_url(url, method, firmware=firmware)
    for url, method, firmware in (
        ("http://account.xiaomi.com/", "GET", False),
        ("https://account.xiaomi.com.evil.test/", "GET", False),
        ("https://c3.lp.account.xiaomi.com.evil.test/", "GET", False),
        ("https://user:secret@account.xiaomi.com/", "GET", False),
        ("https://api.io.mi.com/app/home/rpc/123", "POST", False),
        ("https://api.io.mi.com/app/home/device_list", "POST", False),
        ("https://evil.test/firmware.bin", "GET", True),
        ("https://cdn.cnbj1.fds.api.mi.com.evil.test/firmware.bin", "GET", True),
        ("https://iot-ota-cdn.io.mi.com.evil.test/firmware.bin", "GET", True),
        ("https://account.xiaomi.com/firmware.bin", "GET", True),
        ("https://cdn.cnbj1.fds.api.mi-img.com/firmware.bin", "POST", True),
    ):
        try:
            check_url(url, method, firmware=firmware)
        except DownloadRejected:
            continue
        raise AssertionError("unsafe URL or API was accepted")
    check_version(SimpleNamespace(model="njcuk.enstor.ap01", version="1.0.2_0041"))
    for operation in (
        lambda: check_version(SimpleNamespace(model="njcuk.enstor.ap01", version="1.0.2_0031")),
        lambda: check_md5("a" * 32, "b" * 32),
    ):
        try:
            operation()
        except DownloadRejected:
            continue
        raise AssertionError("invalid version or checksum was accepted")
    # Mock one redirect: check that the actual Session.send path rejects its destination.
    from unittest.mock import patch
    from requests.adapters import HTTPAdapter
    def redirected(adapter, request, **kwargs):
        response = requests.Response()
        response.status_code = 302
        response.headers["Location"] = "https://evil.test/credential-sink"
        response._content = b""
        response.url = request.url
        response.request = request
        return response
    with CheckedSession() as session, patch.object(HTTPAdapter, "send", redirected):
        try:
            session.get("https://account.xiaomi.com/start")
        except DownloadRejected:
            pass
        else:
            raise AssertionError("external redirect was accepted")
    # A failed download retries on the same already-authenticated client.
    import contextlib
    import io
    from unittest.mock import Mock
    client = Mock()
    client.firmware_info.return_value = SimpleNamespace(
        url="https://iot-ota-cdn.io.mi.com/test.bin", public_dict=lambda: {"version": "1.0.2_0041"}
    )
    with patch(__name__ + ".save_rom", side_effect=[requests.Timeout(), {"verified": True}]) as save, \
         patch("builtins.input", return_value="retry"), contextlib.redirect_stdout(io.StringIO()):
        assert download_with_retry(client) == {"verified": True}
        assert client.firmware_info.call_count == 2 and save.call_count == 2
    fake_account = {"userId": "fixture-user", "passToken": "fixture-token", "deviceId": "fixture-device"}
    with tempfile.NamedTemporaryFile(dir=OUTPUT, suffix=".cache-test", delete=False) as stream:
        cache_test = Path(stream.name)
    try:
        with patch(__name__ + ".CREDENTIALS", cache_test):
            save_account(fake_account)
            assert load_account() == fake_account
            assert b"fixture-token" not in cache_test.read_bytes()
            with patch(__name__ + ".save_rom", side_effect=DownloadRejected("fixture failure")), \
                 patch("builtins.input", return_value="cancel"), contextlib.redirect_stdout(io.StringIO()):
                try:
                    download_with_retry(client)
                except DownloadRejected:
                    pass
                else:
                    raise AssertionError("failed download was accepted")
            assert load_account() == fake_account
    finally:
        cache_test.unlink(missing_ok=True)
    print("self-test PASS: HTTPS/domain/API/redirect/version/MD5/retry/persisted login; no network or real credentials")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--login", action="store_true", help="重新扫码并保存登录态；默认复用已保存凭据")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not OUTPUT.is_dir():
        print("先按仓库规则创建 artifacts/ap01-research 目录", flush=True)
        return 2
    qr = OUTPUT / "xiaomi-login-qr.png"
    if qr.exists() and (args.login or not CREDENTIALS.exists()):
        print("已有二维码文件；请确认旧查询进程已退出并清理后重试", flush=True)
        return 2
    try:
        with CheckedSession() as session:
            if CREDENTIALS.exists() and not args.login:
                account = load_account()
                print("复用已保存的米家登录凭据", flush=True)
            else:
                login = XiaomiQrLogin(session=session)
                challenge = login.start(qr, 300)
                print(f"二维码：{qr}\n请用官方米家 App 扫码、核对登录请求后确认（5 分钟内）。", flush=True)
                values = login.wait(challenge)
                account = {
                    "userId": values["CUKTECH_MI_USER_ID"],
                    "passToken": values["CUKTECH_MI_PASS_TOKEN"],
                    "deviceId": values["CUKTECH_MI_DEVICE_ID"],
                }
                save_account(account)
                print("登录凭据已保存并读回校验：" + str(CREDENTIALS), flush=True)
            client = XiaomiCloudClient(account=account, session=session)
            print(json.dumps(download_with_retry(client), ensure_ascii=False, indent=2), flush=True)
        return 0
    except DownloadRejected as exc:
        print("已停止：" + str(exc), flush=True)
    except KeyboardInterrupt:
        print("查询已取消", flush=True)
    except Exception as exc:
        # Upstream exceptions can contain ticket-bearing URLs; never print their messages.
        print(f"查询/下载失败（{type(exc).__name__}）；未发设备更新，错误原文已隐藏。", flush=True)
    finally:
        qr.unlink(missing_ok=True)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
