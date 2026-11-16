import asyncio
import json
import os
import re
import sqlite3
import ssl
import urllib.parse
import urllib.request
from pathlib import Path

try:
    import decky
    HOME = Path(decky.DECKY_USER_HOME)
except ImportError:
    HOME = Path.home()

DB = Path(os.environ.get('XDG_DATA_HOME', HOME / '.local/share')) / 'local.wuwa.tracker/history.sqlite3'
NAMES = [
    '',
    'Ивентовый персонаж',
    'Ивентовое оружие',
    'Стандартный персонаж',
    'Стандартное оружие',
    'Новичок',
    'Выбор новичка',
    'Выбор новичка · возврат',
    'Другой баннер',
    '',
    '',
    '',
    'Отголоски · персонаж',
    'Отголоски · оружие',
]
URL_RE = re.compile(r'https?://[^\s"\'\]\)]+/aki/gacha/index\.html#/record\?[^\s"\'\]\),]+')
VDF_PATH_RE = re.compile(r'"path"\s+"((?:\\.|[^"\\])*)"')

DIRECT_LOG_SUBPATHS = (
    'Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs',
    'Wuthering Waves/Client/Saved/Logs',
)

WINE_PREFIX_SUBPATHS = (
    'Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs',
    'Wuthering Waves/Client/Saved/Logs',
    'Wuthering Waves Game/Client/Saved/Logs',
    'Program Files/Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs',
    'Program Files/Wuthering Waves/Client/Saved/Logs',
    'Program Files/Epic Games/WutheringWavesj3oFh/Wuthering Waves Game/Client/Saved/Logs',
    'Program Files/Epic Games/WutheringWavesj3oFh/Client/Saved/Logs',
    'Games/Wuthering Waves/Wuthering Waves Game/Client/Saved/Logs',
)

SKIP_WALK_DIRS = {
    'dosdevices',
    'compatdata',
    'shadercache',
    'depotcache',
    'downloading',
    'temp',
    'workshop',
    'crashes',
    'aftermathshaderdebuginfo_backup',
    'ngx',
    'windows',
    'system32',
    'syswow64',
    'cache',
    'gpucache',
    'code cache',
    'ebwebview',
    'krwebviewuserdata',
    '.git',
    'node_modules',
}


def database():
    DB.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(DB, timeout=15)
    con.execute(
        'CREATE TABLE IF NOT EXISTS pulls ('
        'player TEXT NOT NULL, pool INTEGER NOT NULL, time TEXT NOT NULL, '
        'seq INTEGER NOT NULL, name TEXT NOT NULL, quality INTEGER NOT NULL, '
        'PRIMARY KEY(player,pool,time,seq))'
    )
    return con


def stats():
    with database() as con:
        rows = con.execute(
            'SELECT player,pool,time,seq,name,quality FROM pulls ORDER BY player,pool,time,seq'
        ).fetchall()
    groups = {}
    for player, pool, time, seq, name, quality in rows:
        k = (player, pool)
        if k not in groups:
            groups[k] = {
                'player': player,
                'pool': pool,
                'label': NAMES[pool] if 0 < pool < len(NAMES) else str(pool),
                'total': 0,
                'five': 0,
                'pity': 0,
                'known': False,
                'records': [],
            }
        g = groups[k]
        g['total'] += 1
        g['pity'] += 1
        g['records'].append({'name': name, 'time': time, 'quality': quality, 'seq': seq})
        if quality == 5:
            g['five'] += 1
            g['pity'] = 0
            g['known'] = True
    for g in groups.values():
        g['records'].reverse()
    return list(groups.values())


def parse_libraryfolders_vdf(text: str) -> list[Path]:
    libs: list[Path] = []
    seen: set[Path] = set()
    for m in VDF_PATH_RE.finditer(text):
        raw = m.group(1).replace('\\\\', '\\').replace('\\"', '"').strip()
        if not raw:
            continue
        p = Path(raw)
        sa = p if p.name.lower() == 'steamapps' else p / 'steamapps'
        if sa not in seen:
            seen.add(sa)
            libs.append(sa)
    return libs


def _collect_heroic_roots(home: Path) -> list[Path]:
    roots: list[Path] = []
    configs = [
        home / '.config/heroic',
        home / '.var/app/com.heroicgameslauncher.hgl/config/heroic',
    ]
    for cfg in configs:
        for rel in ('installed.json', 'store_cache/legendary_library.json', 'sideload_apps/library.json'):
            f = cfg / rel
            if not f.is_file():
                continue
            try:
                data = json.loads(f.read_text('utf-8', errors='replace'))
            except (OSError, ValueError):
                continue
            items = data if isinstance(data, list) else (data.get('installed') or data.get('games') or [])
            if isinstance(items, dict):
                items = list(items.values())
            if not isinstance(items, list):
                continue
            for entry in items:
                if not isinstance(entry, dict):
                    continue
                for key in ('install_path', 'folder_name', 'winePrefix'):
                    val = entry.get(key)
                    if isinstance(val, str) and val.strip():
                        roots.append(Path(val.strip()))
        games_cfg = cfg / 'GamesConfig'
        if games_cfg.is_dir():
            for jf in games_cfg.glob('*.json'):
                try:
                    data = json.loads(jf.read_text('utf-8', errors='replace'))
                except (OSError, ValueError):
                    continue
                if isinstance(data, dict):
                    for v in data.values():
                        if isinstance(v, dict):
                            wp = v.get('winePrefix')
                            if isinstance(wp, str) and wp.strip():
                                roots.append(Path(wp.strip()))
    return roots


def _collect_lutris_roots(home: Path) -> list[Path]:
    roots: list[Path] = []
    configs = [
        home / '.config/lutris/games',
        home / '.var/app/net.lutris.Lutris/config/lutris/games',
    ]
    for cfg in configs:
        if not cfg.is_dir():
            continue
        for yml in cfg.glob('*.yml'):
            try:
                for line in yml.read_text('utf-8', errors='replace').splitlines():
                    stripped = line.strip()
                    for key in ('exe:', 'prefix:', 'working_dir:'):
                        if stripped.startswith(key):
                            val = stripped[len(key):].strip().strip('"\'')
                            if val:
                                p = Path(val)
                                roots.append(p.parent if key == 'exe:' else p)
            except OSError:
                continue
    return roots


def _is_saved_logs_file(path: Path) -> bool:
    if path.suffix.lower() != '.log':
        return False
    parts = [p.lower() for p in path.parts]
    if 'crashes' in parts:
        return False
    return any(parts[i] == 'saved' and parts[i + 1] == 'logs' for i in range(len(parts) - 1))


def find_log_files(home: Path | None = None) -> list[Path]:
    if home is None:
        home = HOME

    xdg_data = Path(os.environ.get('XDG_DATA_HOME', str(home / '.local/share')))
    steam_roots = [
        home / '.local/share/Steam',
        home / '.steam/steam',
        home / '.steam/root',
        home / '.steam/debian-installation',
        home / 'steam',
        home / 'Steam',
        home / '.var/app/com.valvesoftware.Steam/.local/share/Steam',
        home / '.var/app/com.valvesoftware.Steam/.steam/steam',
        home / 'snap/steam/common/.local/share/Steam',
        xdg_data / 'Steam',
    ]

    steamapps_dirs: list[Path] = []
    seen_sa: set[Path] = set()

    def add_steamapps(sa: Path):
        if sa not in seen_sa and sa.is_dir():
            seen_sa.add(sa)
            steamapps_dirs.append(sa)

    for sr in steam_roots:
        add_steamapps(sr / 'steamapps')
        for vdf in (sr / 'steamapps/libraryfolders.vdf', sr / 'config/libraryfolders.vdf'):
            if vdf.is_file():
                try:
                    for lib_sa in parse_libraryfolders_vdf(vdf.read_text('utf-8', errors='replace')):
                        add_steamapps(lib_sa)
                except OSError:
                    pass

    # Check removable / SD card / external mounts up to depth 2
    mount_candidates: list[Path] = []
    for base in (Path('/run/media'), Path('/media'), Path('/mnt')):
        if not base.is_dir():
            continue
        try:
            for d1 in base.iterdir():
                if not d1.is_dir():
                    continue
                mount_candidates.append(d1)
                try:
                    for d2 in d1.iterdir():
                        if d2.is_dir():
                            mount_candidates.append(d2)
                except OSError:
                    pass
        except OSError:
            pass

    for mc in mount_candidates:
        add_steamapps(mc / 'steamapps')
        add_steamapps(mc / 'SteamLibrary/steamapps')

    direct_log_dirs: list[Path] = []
    walk_roots: list[Path] = []

    for sa in steamapps_dirs:
        common = sa / 'common'
        for sub in DIRECT_LOG_SUBPATHS:
            direct_log_dirs.append(common / sub)
        walk_roots.append(common)

        compat = sa / 'compatdata'
        if compat.is_dir():
            try:
                for app_dir in compat.iterdir():
                    drive_c = app_dir / 'pfx/drive_c'
                    if not drive_c.is_dir():
                        continue
                    for sub in WINE_PREFIX_SUBPATHS:
                        direct_log_dirs.append(drive_c / sub)
                    direct_log_dirs.append(drive_c / 'users/steamuser/AppData/Local/KR_G152/Saved/Logs')
            except OSError:
                pass

    extra_roots = [
        home / 'Games',
        home / '.local/share/bottles/bottles',
        home / '.var/app/com.usebottles.bottles/data/bottles/bottles',
        *_collect_heroic_roots(home),
        *_collect_lutris_roots(home),
    ]
    for mc in mount_candidates:
        for name in ('Wuthering Waves', 'Wuthering Waves Game', 'Games'):
            p = mc / name
            if p.is_dir():
                extra_roots.append(p)

    for root in extra_roots:
        if not root.is_dir():
            continue
        direct_log_dirs.append(root / 'Wuthering Waves Game/Client/Saved/Logs')
        direct_log_dirs.append(root / 'Client/Saved/Logs')
        drive_c = root / 'drive_c'
        if drive_c.is_dir():
            for sub in WINE_PREFIX_SUBPATHS:
                direct_log_dirs.append(drive_c / sub)
        walk_roots.append(root)

    found: dict[Path, tuple[float, bool, Path]] = {}

    def record_log(p: Path):
        if not p.is_file() or not _is_saved_logs_file(p):
            return
        try:
            canon = p.resolve()
            st = canon.stat()
        except OSError:
            return
        if canon not in found:
            found[canon] = (st.st_mtime, p.name.lower() == 'client.log', p)

    for d in direct_log_dirs:
        if not d.is_dir():
            continue
        try:
            for item in d.iterdir():
                if item.suffix.lower() == '.log':
                    record_log(item)
        except OSError:
            pass

    seen_dirs: set[tuple[int, int]] = set()
    for root in walk_roots:
        if not root.is_dir():
            continue
        base_depth = len(root.parts)
        for dirpath, dirnames, filenames in os.walk(root, followlinks=True):
            cur = Path(dirpath)
            try:
                st = cur.stat()
                key = (st.st_dev, st.st_ino)
            except OSError:
                dirnames[:] = []
                continue
            if key in seen_dirs:
                dirnames[:] = []
                continue
            seen_dirs.add(key)

            depth = len(cur.parts) - base_depth
            if depth >= 8:
                dirnames[:] = []
            else:
                dirnames[:] = [d for d in dirnames if d.lower() not in SKIP_WALK_DIRS]

            for fn in filenames:
                if fn.lower().endswith('.log'):
                    record_log(cur / fn)

    ordered = sorted(found.values(), key=lambda x: (x[0], x[1]), reverse=True)
    return [item[2] for item in ordered]


def extract_url_from_bytes(raw: bytes) -> str | None:
    best_end = -1
    best_url: str | None = None
    decoded = bytes(b ^ (0xa5 if b & 1 else 0xef) for b in raw)
    for content in (raw, decoded):
        text = content.decode('utf-8', 'replace')
        for m in URL_RE.finditer(text):
            if m.end() >= best_end:
                best_end = m.end()
                best_url = m.group(0)
    return best_url


def find_url(home: Path | None = None) -> str:
    for path in find_log_files(home):
        try:
            raw = path.read_bytes()
        except OSError:
            continue
        link = extract_url_from_bytes(raw)
        if link:
            return link
    raise ValueError('Ссылка не найдена. Откройте историю круток в игре и попробуйте снова.')


def sync():
    link = find_url()
    u = urllib.parse.urlsplit(link)
    if u.scheme != 'https' or not (
        u.hostname and (u.hostname.endswith('.aki-game.net') or u.hostname.endswith('.aki-game.com'))
    ):
        raise ValueError('Некорректный адрес истории.')
    q = dict(urllib.parse.parse_qsl(u.fragment.partition('?')[2]))
    needed = ['player_id', 'record_id', 'svr_id', 'resources_id']
    if any(not q.get(k) for k in needed):
        raise ValueError('В ссылке отсутствуют параметры.')
    endpoint = (
        'https://gmserver-api.aki-game2.com/gacha/record/query'
        if q.get('svr_area') == 'cn'
        else 'https://gmserver-api.aki-game2.net/gacha/record/query'
    )
    all_rows = []
    for pool in (1, 2, 3, 4, 5, 6, 7, 8, 12, 13):
        body = {
            'playerId': q['player_id'],
            'recordId': q['record_id'],
            'serverId': q['svr_id'],
            'cardPoolId': q['resources_id'],
            'cardPoolType': pool,
            'languageCode': q.get('lang', 'en'),
        }
        req = urllib.request.Request(
            endpoint,
            data=json.dumps(body).encode(),
            headers={'Content-Type': 'application/json', 'User-Agent': 'Mozilla/5.0'},
        )

        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, req, fp, code, msg, headers, newurl):
                return None

        context = ssl.create_default_context(cafile='/etc/ssl/certs/ca-certificates.crt')
        opener = urllib.request.build_opener(NoRedirect, urllib.request.HTTPSHandler(context=context))
        with opener.open(req, timeout=30) as resp:
            result = json.load(resp)
        if result.get('code') != 0:
            raise ValueError(
                'API отклонил запрос (код %s). Откройте историю в игре заново и сразу повторите.'
                % result.get('code')
            )
        counts = {}
        for item in reversed(result.get('data') or []):
            t = item['time']
            counts[t] = counts.get(t, 0) + 1
            all_rows.append((q['player_id'], pool, t, counts[t], item['name'], int(item['qualityLevel'])))
    with database() as con:
        old = con.total_changes
        con.executemany('INSERT OR IGNORE INTO pulls VALUES (?,?,?,?,?,?)', all_rows)
        return con.total_changes - old


class Plugin:
    async def get_stats(self):
        try:
            return {'ok': True, 'data': await asyncio.to_thread(stats)}
        except Exception as e:
            return {'ok': False, 'error': str(e)}

    async def update_history(self):
        try:
            count = await asyncio.to_thread(sync)
            return {'ok': True, 'added': count, 'data': await asyncio.to_thread(stats)}
        except Exception as e:
            return {'ok': False, 'error': str(e)}
