import asyncio
import json
import os
import re
import sqlite3
import ssl
import urllib.parse
import urllib.request
from pathlib import Path
import decky

HOME = Path(decky.DECKY_USER_HOME)
DB = Path(os.environ.get('XDG_DATA_HOME', HOME / '.local/share')) / 'local.wuwa.tracker/history.sqlite3'
NAMES = ['', 'Ивентовый персонаж', 'Ивентовое оружие', 'Стандартный персонаж', 'Стандартное оружие', 'Новичок', 'Выбор новичка', 'Выбор новичка · возврат', 'Другой баннер', '', '', '', 'Отголоски · персонаж', 'Отголоски · оружие']
URL_RE = re.compile(r'https?://[^\s"\'\]\)]+/aki/gacha/index\.html#/record\?[^\s"\'\]\),]+')

def database():
    DB.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(DB, timeout=15)
    con.execute('CREATE TABLE IF NOT EXISTS pulls (player TEXT NOT NULL, pool INTEGER NOT NULL, time TEXT NOT NULL, seq INTEGER NOT NULL, name TEXT NOT NULL, quality INTEGER NOT NULL, PRIMARY KEY(player,pool,time,seq))')
    return con

def stats():
    with database() as con:
        rows = con.execute('SELECT player,pool,time,seq,name,quality FROM pulls ORDER BY player,pool,time,seq').fetchall()
    groups = {}
    for player,pool,time,seq,name,quality in rows:
        k=(player,pool)
        if k not in groups: groups[k]={'player':player,'pool':pool,'label':NAMES[pool] if 0 < pool < len(NAMES) else str(pool),'total':0,'five':0,'pity':0,'known':False,'records':[]}
        g=groups[k]; g['total']+=1; g['pity']+=1; g['records'].append({'name':name,'time':time,'quality':quality,'seq':seq})
        if quality==5:
            g['five']+=1; g['pity']=0; g['known']=True
    for g in groups.values(): g['records'].reverse()
    return list(groups.values())

def sync():
    roots=[HOME/'.local/share/Steam/steamapps/common', HOME/'.steam/steam/steamapps/common']
    logs=[]
    for root in roots:
        if root.exists(): logs.extend(root.glob('Wuthering Waves/**/Client/Saved/Logs/*.log'))
    logs=sorted(set(logs), key=lambda p:p.stat().st_mtime, reverse=True)
    link=None
    for path in logs:
        try: raw=path.read_bytes()
        except OSError: continue
        for content in (raw, bytes(b ^ (0xa5 if b & 1 else 0xef) for b in raw)):
            matches=list(URL_RE.finditer(content.decode('utf-8','replace')))
            if matches: link=matches[-1].group(); break
        if link: break
    if not link: raise ValueError('Ссылка не найдена. Откройте историю круток в игре и попробуйте снова.')
    u=urllib.parse.urlsplit(link)
    if u.scheme!='https' or not (u.hostname and (u.hostname.endswith('.aki-game.net') or u.hostname.endswith('.aki-game.com'))): raise ValueError('Некорректный адрес истории.')
    q=dict(urllib.parse.parse_qsl(u.fragment.partition('?')[2])); needed=['player_id','record_id','svr_id','resources_id']
    if any(not q.get(k) for k in needed): raise ValueError('В ссылке отсутствуют параметры.')
    endpoint='https://gmserver-api.aki-game2.com/gacha/record/query' if q.get('svr_area')=='cn' else 'https://gmserver-api.aki-game2.net/gacha/record/query'
    all_rows=[]
    for pool in (1,2,3,4,5,6,7,8,12,13):
        body={'playerId':q['player_id'],'recordId':q['record_id'],'serverId':q['svr_id'],'cardPoolId':q['resources_id'],'cardPoolType':pool,'languageCode':q.get('lang','en')}
        req=urllib.request.Request(endpoint,data=json.dumps(body).encode(),headers={'Content-Type':'application/json','User-Agent':'Mozilla/5.0'})
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, req, fp, code, msg, headers, newurl): return None
        context = ssl.create_default_context(cafile='/etc/ssl/certs/ca-certificates.crt')
        opener = urllib.request.build_opener(NoRedirect, urllib.request.HTTPSHandler(context=context))
        with opener.open(req,timeout=30) as resp: result=json.load(resp)
        if result.get('code') != 0: raise ValueError('API отклонил запрос (код %s). Откройте историю в игре заново и сразу повторите.' % result.get('code'))
        counts={}
        for item in reversed(result.get('data') or []):
            t=item['time']; counts[t]=counts.get(t,0)+1
            all_rows.append((q['player_id'],pool,t,counts[t],item['name'],int(item['qualityLevel'])))
    with database() as con:
        old=con.total_changes
        con.executemany('INSERT OR IGNORE INTO pulls VALUES (?,?,?,?,?,?)',all_rows)
        return con.total_changes-old

class Plugin:
    async def get_stats(self):
        try: return {'ok':True,'data':await asyncio.to_thread(stats)}
        except Exception as e: return {'ok':False,'error':str(e)}
    async def update_history(self):
        try:
            count=await asyncio.to_thread(sync)
            return {'ok':True,'added':count,'data':await asyncio.to_thread(stats)}
        except Exception as e: return {'ok':False,'error':str(e)}
