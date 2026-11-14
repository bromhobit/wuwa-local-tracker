const connection = window.__DECKY_SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED_deckyLoaderAPIInit;
let api; try { api=connection.connect(2,'WuWa Tracker'); } catch { api=connection.connect(1,'WuWa Tracker'); }
const e=SP_REACT.createElement;
const labels={1:'Ивентовый персонаж',2:'Ивентовое оружие',3:'Стандартный персонаж',4:'Стандартное оружие',8:'Другой баннер',12:'Отголоски · персонаж',13:'Отголоски · оружие'};
const sections=[{label:'Персонаж',data:'1'},{label:'Оружие',data:'2'},{label:'Стандарт',data:'standard'},{label:'Отголоски · персонаж',data:'12'},{label:'Отголоски · оружие',data:'13'}];
function Content(){
 const container=SP_REACT.useRef(null), scrollSnapshot=SP_REACT.useRef(null);
 function rememberScroll(){let el=container.current;while(el){const style=getComputedStyle(el);if(/auto|scroll/.test(style.overflowY)&&el.scrollHeight>el.clientHeight){scrollSnapshot.current={el,top:el.scrollTop};return}el=el.parentElement}}
 const [data,setData]=SP_REACT.useState([]),[status,setStatus]=SP_REACT.useState(''),[busy,setBusy]=SP_REACT.useState(false),[player,setPlayer]=SP_REACT.useState(''),[section,setSection]=SP_REACT.useState('1'),[page,setPage]=SP_REACT.useState(0);
 SP_REACT.useLayoutEffect(()=>{const snap=scrollSnapshot.current;if(!snap)return;scrollSnapshot.current=null;const restore=()=>{if(snap.el.isConnected)snap.el.scrollTop=snap.top};restore();requestAnimationFrame(()=>{restore();setTimeout(restore,80)})},[section,player,page]);
 async function run(method){setBusy(true);setStatus(method==='update_history'?'Загрузка истории…':'Загрузка…');try{const r=await api.call(method);if(!r.ok)throw new Error(r.error);setData(r.data);setPage(0);setStatus(method==='update_history'?`Добавлено записей: ${r.added}`:'');}catch(err){setStatus(String(err.message||err));}finally{setBusy(false)}}
 SP_REACT.useEffect(()=>{run('get_stats')},[]);
 const players=[...new Set(data.map(g=>g.player))];const selected=players.includes(player)?player:(players[0]||'');const groups=data.filter(g=>g.player===selected&&[1,2,3,4,8,12,13].includes(g.pool));
 const matching=groups.filter(g=>section==='standard'?[3,4,8].includes(g.pool):String(g.pool)===section);
 const records=matching.flatMap(g=>(g.records||[]).map(r=>({...r,pool:g.pool}))).sort((a,b)=>b.time.localeCompare(a.time)||(b.seq||0)-(a.seq||0)||a.pool-b.pool);
 const pages=Math.max(1,Math.ceil(records.length/5)),currentPage=Math.min(page,pages-1);
 const row=(child,key)=>e(DFL.PanelSectionRow,{key},child),button=(label,action,disabled=false)=>e(DFL.ButtonItem,{layout:'below',disabled,onClick:action},label);
 const cards=selected?[1,2,3,4,12,13,...(groups.some(g=>g.pool===8)?[8]:[])].map(pool=>{
  const g=groups.find(x=>x.pool===pool);
  const pity=g?(g.known?String(g.pity):`не менее ${g.pity}`):'—';
  const remaining=g&&g.known&&[1,2,3,4,12,13].includes(pool)&&g.pity<80?String(80-g.pity):'—';
  return row(e('div',{style:{width:'100%',padding:'8px 0',borderBottom:'1px solid #444',overflowWrap:'anywhere'}},
   e('strong',null,labels[pool]),
   e('div',null,g?`${g.total} круток · ${g.five} × 5★`:'0 круток · 0 × 5★'),
   e('div',null,`Откручено: ${pity}`),
   e('div',null,`До гаранта 5★: ${remaining}`),
   g&&!g.known&&e('div',{style:{fontSize:'0.85em'}},'История неполная: счётчик может быть выше.')
  ),'card'+pool);
 }):[];
 return e('div',{ref:container},e(DFL.PanelSection,null,
 row(e('div',null,'Откройте историю круток в игре, затем нажмите кнопку ниже.'),'hint'),
 row(button(busy?'Подождите…':'Обновить историю',()=>run('update_history'),busy),'update'),
 status&&row(e('div',{style:{whiteSpace:'normal',overflowWrap:'anywhere'}},status),'status'),
 selected&&row(e(DFL.DropdownItem,{label:'Аккаунт',rgOptions:players.map(p=>({label:`UID ${p}`,data:p})),selectedOption:selected,onChange:option=>{rememberScroll();setPlayer(option.data);setPage(0)}}),'account'),
 !selected&&!busy&&row(e('div',null,'Записей: 0'),'empty'),
 ...cards,
 row(e('strong',null,'История'),'history-title'),
 row(button(`Баннер: ${sections.find(x=>x.data===section)?.label || sections[0].label} · сменить`,()=>{rememberScroll();const index=sections.findIndex(x=>x.data===section);setSection(sections[(index+1)%sections.length].data);setPage(0)}),'section'),
 row(e('div',{style:{width:'100%',minHeight:'300px',overflowWrap:'anywhere'}},
   records.length===0?e('div',null,'Записей: 0'):
   records.slice(currentPage*5,currentPage*5+5).map((r,i)=>e('div',{key:`record${i}`,style:{width:'100%',padding:'7px 0',borderBottom:'1px solid #444',overflowWrap:'anywhere'}},e('strong',{style:{color:r.quality===5?'#f4c572':r.quality===4?'#d1a0ff':'#81bfff'}},r.name),e('div',null,`${'★'.repeat(r.quality)} · ${labels[r.pool]}`),e('div',{style:{fontSize:'0.85em'}},r.time)))
 ),'records'),
 row(e('div',{style:{textAlign:'center',width:'100%'}},`Страница ${currentPage+1} из ${pages}`),'page'),
 row(button('← Назад',()=>{rememberScroll();setPage(currentPage-1)},currentPage===0),'prev'),
 row(button('Вперёд →',()=>{rememberScroll();setPage(currentPage+1)},currentPage>=pages-1),'next')
 ));
}
const plugin=()=>({name:'WuWa Tracker',titleView:e('div',{className:DFL.staticClasses.Title},'WuWa Tracker'),content:e(Content),icon:e('span',null,'✦'),onDismount(){}});
export {plugin as default};
