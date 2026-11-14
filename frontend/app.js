const { invoke } = window.__TAURI_INTERNALS__;
const $ = s => document.querySelector(s);
const translations={
 ru:{names:['','Ивентовый персонаж','Ивентовое оружие','Стандартный персонаж','Стандартное оружие','Новичок','Выбор новичка','Выбор новичка · возврат','Другой баннер'],static:['ЛИЧНЫЙ АРХИВ · WUTHERING WAVES','История круток','Ваши данные хранятся на этом компьютере.','Экспорт JSON','Импорт JSON','Обновить историю','Откройте историю круток в игре, затем найдите ссылку в логах или вставьте её вручную.','Найти в Steam','Обновить','Ваша статистика','Счётчики основаны на сохранённой истории; при неполной истории счётчик может быть выше.','Аккаунт','История','Персонаж','Оружие','Стандарт','Дата','Баннер','Предмет','Редкость','В этом разделе нет записей.','← Назад','Вперёд →','Гарант 5★ — максимальное число круток до предмета редкости 5★. Гарант ивентового персонажа после 50/50 определяется отдельно и пока не отслеживается.'],placeholder:'Ссылка истории круток',none:'Нет аккаунтов',pulls:'круток',pulled:'Откручено: ',remaining:'До гаранта 5★',incomplete:'История неполная: счётчик может быть выше',average:'Среднее между 5★',page:(p,n)=>`Страница ${p} из ${n}`,working:'Выполняется…',found:'Ссылка найдена. Нажмите «Обновить».',synced:n=>`Импорт завершён: новых записей ${n}.`,exported:'Резервная копия сохранена.',imported:n=>`Импортировано записей: ${n}.`},
 en:{names:['','Featured character','Featured weapon','Standard character','Standard weapon','Novice','Novice choice','Novice choice · return','Other banner'],static:['PERSONAL ARCHIVE · WUTHERING WAVES','Convene history','Your data is stored on this computer.','Export JSON','Import JSON','Update history','Open Convene History in the game, then find the link in the logs or paste it manually.','Find in Steam','Update','Your statistics','Counters are based on saved history; incomplete history may mean a higher count.','Account','History','Character','Weapon','Standard','Date','Banner','Item','Rarity','No records in this section.','← Previous','Next →','5★ guarantee is the maximum number of pulls until a 5★ item. The featured character guarantee after a 50/50 is separate and is not tracked yet.'],placeholder:'Convene history link',none:'No accounts',pulls:'pulls',pulled:'Pulls since 5★: ',remaining:'Until guaranteed 5★',incomplete:'History is incomplete; the count may be higher',average:'Average between 5★ pulls',page:(p,n)=>`Page ${p} of ${n}`,working:'Working…',found:'Link found. Select Update.',synced:n=>`Import complete: ${n} new records.`,exported:'Backup saved.',imported:n=>`Imported ${n} records.`}
};
let language=localStorage.getItem('wuwa-language')==='en'?'en':'ru';
const t=()=>translations[language];
const staticSelectors=['header small','header h1','header p','#export','#import-label','.import h2','.import p:not(#status)','#find','#sync','.toolbar h2','.toolbar p','.account-label','.history-section h2','[data-section="1"]','[data-section="2"]','[data-section="other"]','thead th:nth-child(1)','thead th:nth-child(2)','thead th:nth-child(3)','thead th:nth-child(4)','#empty','#prev','#next','footer'];
function translate(){document.documentElement.lang=language;staticSelectors.forEach((selector,i)=>$(selector).textContent=t().static[i]);$('#link').placeholder=t().placeholder;$('#language-current').textContent=language==='en'?'English':'Русский';document.querySelectorAll('[data-language]').forEach(b=>b.setAttribute('aria-selected',String(b.dataset.language===language)));$('#account-options').setAttribute('aria-label',language==='en'?'Choose account':'Выбор аккаунта');document.querySelector('.tabs').setAttribute('aria-label',language==='en'?'Banner type':'Тип баннера');renderAccounts();render()}

let pulls=[], section='1', page=0, selectedPlayer='';
function render(){
 const player=selectedPlayer, data=pulls.filter(p=>p.player===player); $('#stats').replaceChildren();
 for(const pool of [1,2,3,4,8]){
  const list=data.filter(p=>p.pool===pool);if(!list.length)continue;let count=0,fives=0,sum=0,complete=0,seen=false;
  for(const p of list){count++;if(p.quality===5){fives++;if(seen){sum+=count;complete++}seen=true;count=0}}
  const card=document.createElement('article'),h=document.createElement('h3'),description=document.createElement('p'),strong=document.createElement('strong'),hint=document.createElement('p'),average=document.createElement('p');
  h.textContent=t().names[pool];description.textContent=`${list.length} ${t().pulls} · ${fives} × 5★`;strong.textContent=`${seen?'':'≥ '}${count}`;hint.textContent=t().pulled;hint.className='pull-count-label';strong.className='pull-count-value';
  average.textContent=seen&&pool<=4&&count<80?`${t().remaining}: ${80-count}` : seen?'':t().incomplete;
  const avg=document.createElement('p');avg.textContent=`${t().average}: ${complete?(sum/complete).toFixed(1):'—'}`;const counter=document.createElement('div');counter.className='pull-count';counter.append(hint,strong);card.append(h,description,counter,average,avg);$('#stats').append(card);
 }
 renderHistory();
}
function renderHistory(){
 const data=pulls.filter(p=>p.player===selectedPlayer);$('#history').replaceChildren();
 const list=data.filter(p=>section==='other'?[3,4,8].includes(p.pool):String(p.pool)===section).sort((a,b)=>b.time.localeCompare(a.time)||b.seq-a.seq||a.pool-b.pool);
 const pages=Math.max(1,Math.ceil(list.length/5));page=Math.min(page,pages-1);$('#empty').hidden=!!list.length;
 for(const p of list.slice(page*5,page*5+5)){const tr=document.createElement('tr');for(const [i,value] of [p.time,t().names[p.pool],p.name,'★'.repeat(p.quality)].entries()){const td=document.createElement('td');td.textContent=value;if(i===2||i===3)td.classList.add(`rarity-${p.quality}`);tr.append(td)}$('#history').append(tr)}
 $('#page').textContent=t().page(page+1,pages);$('#prev').disabled=page===0;$('#next').disabled=page>=pages-1;
 document.querySelectorAll('[data-section]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.section===section)));
}
function renderAccounts(){
 const players=[...new Set(pulls.map(p=>p.player))];if(!players.includes(selectedPlayer))selectedPlayer=players[0]||'';
 $('#account-current').textContent=selectedPlayer?`UID ${selectedPlayer}`:t().none;$('#account-toggle').disabled=!players.length;
 $('#account-options').replaceChildren(...players.map(p=>{const b=document.createElement('button');b.type='button';b.className='account-option';b.setAttribute('role','option');b.setAttribute('aria-selected',String(p===selectedPlayer));const label=document.createElement('span');label.textContent=`UID ${p}`;const mark=document.createElement('span');mark.textContent=p===selectedPlayer?'✓':'';b.append(label,mark);b.onclick=()=>{selectedPlayer=p;page=0;closeAccounts();renderAccounts();render();$('#account-toggle').focus()};return b}));
}
function closeAccounts(){$('#account-options').hidden=true;$('#account-toggle').setAttribute('aria-expanded','false')}
async function load(){pulls=await invoke('history');renderAccounts();render()}
function displayError(error){const message=String(error);if(language!=='en')return message;const known={'Ссылка не найдена. Откройте историю круток в игре или вставьте ссылку вручную.':'Link not found. Open Convene History in the game or paste the link manually.','Ошибка соединения с игровым API':'Could not connect to the game API.','Ошибка HTTP игрового API':'Game API HTTP error.','Не удалось прочитать ответ API':'Could not read the API response.','Некорректная ссылка':'Invalid link.','Ожидается ссылка истории Kuro Games':'Expected a Kuro Games history link.','Некорректная запись в файле':'Invalid record in backup.'};if(known[message])return known[message];if(message.startsWith('API отклонил запрос'))return 'The game API rejected the request. Open Convene History in the game, wait for it to load, then find the link and update again.';return message}
async function action(fn){try{$('#status').textContent=t().working;await fn()}catch(e){$('#status').textContent=displayError(e)}}
$('#account-toggle').onclick=()=>{const open=$('#account-options').hidden;$('#account-options').hidden=!open;$('#account-toggle').setAttribute('aria-expanded',String(open))};
document.addEventListener('click',e=>{if(!e.target.closest('.account-picker'))closeAccounts()});
document.addEventListener('keydown',e=>{if(e.key==='Escape'&&!$('#account-options').hidden){closeAccounts();$('#account-toggle').focus()}});
document.querySelectorAll('[data-section]').forEach(b=>b.onclick=()=>{section=b.dataset.section;page=0;renderHistory()});
$('#prev').onclick=()=>{page=Math.max(0,page-1);renderHistory()};$('#next').onclick=()=>{page++;renderHistory()};
$('#find').onclick=()=>action(async()=>{$('#link').value=await invoke('find_url');$('#status').textContent=t().found});
$('#sync').onclick=()=>action(async()=>{const n=await invoke('sync',{link:$('#link').value});$('#link').value='';await load();$('#status').textContent=t().synced(n)});
$('#export').onclick=()=>action(async()=>{const blob=new Blob([JSON.stringify(pulls,null,2)],{type:'application/json'});const a=document.createElement('a');a.href=URL.createObjectURL(blob);a.download='wuwa-history.json';a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);$('#status').textContent=t().exported});
$('#file').onchange=()=>action(async()=>{const file=$('#file').files[0];if(!file)return;const n=await invoke('import_backup',{data:await file.text()});await load();$('#file').value='';$('#status').textContent=t().imported(n)});
function closeLanguage(){$('#language-options').hidden=true;$('#language-toggle').setAttribute('aria-expanded','false')}
$('#language-toggle').onclick=()=>{const open=$('#language-options').hidden;closeAccounts();$('#language-options').hidden=!open;$('#language-toggle').setAttribute('aria-expanded',String(open))};
document.querySelectorAll('[data-language]').forEach(b=>b.onclick=()=>{language=b.dataset.language;localStorage.setItem('wuwa-language',language);$('#status').textContent='';closeLanguage();translate();$('#language-toggle').focus()});
document.addEventListener('click',e=>{if(!e.target.closest('.language-control'))closeLanguage()});
document.addEventListener('keydown',e=>{if(e.key==='Escape'&&!$('#language-options').hidden){closeLanguage();$('#language-toggle').focus()}});
translate();
load().catch(e=>$('#status').textContent=displayError(e));
