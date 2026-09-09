'use strict';
const $=id=>document.getElementById(id);
let token=sessionStorage.getItem('madari-tv-token')||'', model=null, busy=false, editorAction=null;
function node(tag,text,className){const e=document.createElement(tag);if(text!=null)e.textContent=text;if(className)e.className=className;return e;}
function message(text,error=false){$('message').textContent=text;$('message').hidden=!text;$('message').classList.toggle('error',error);}
function setBusy(value){busy=value;document.querySelectorAll('button').forEach(b=>b.disabled=value);document.body.setAttribute('aria-busy',String(value));}
async function api(path,args){const headers={};if(token)headers.Authorization=`Bearer ${token}`;if(args!==undefined)headers['Content-Type']='application/json';const response=await fetch(path,{method:args===undefined?'GET':'POST',headers,body:args===undefined?undefined:JSON.stringify(args),cache:'no-store'});let data;try{data=await response.json();}catch{throw Error('The TV returned an unexpected response. Try refreshing.');}if(!response.ok){if(response.status===401&&path!='/api/pair'){token='';sessionStorage.removeItem('madari-tv-token');showPairing();}throw Error(data.error||'The TV could not save this change.');}return data;}
async function task(fn){if(busy)return;setBusy(true);message('');try{await fn();}catch(e){message(e.message||'Could not reach the TV. Keep Madari open and check Wi-Fi.',true);if($('editor').open){$('editor-error').textContent=e.message;$('editor-error').hidden=false;}}finally{setBusy(false);}}
function showPairing(){$('pairing').hidden=false;$('dashboard').hidden=true;model=null;}
async function refresh(){model=await api('/api/overview');render();}
async function command(operation,args={}){model=await api('/api/command',{operation,args});render();message('Saved to your TV.');}
function button(label,action,cls){const b=node('button',label,cls);b.type='button';b.addEventListener('click',action);return b;}
function field(label,name,{value='',type='text',required=false,options=null,help='',pattern=null}={}){const wrapper=node('label',label);const input=document.createElement(options?'select':'input');input.name=name;if(options){for(const [value,text]of options){const option=node('option',text);option.value=value;input.append(option);}}else{input.type=type;input.autocomplete=type==='password'?'new-password':'off';if(type==='password'){input.inputMode='numeric';input.maxLength=8;if(pattern)input.pattern=pattern;}if(type==='text')input.maxLength=name==='name'?40:4096;}if(type==='checkbox'){wrapper.className='check';input.checked=!!value;wrapper.prepend(input);}else{input.value=value;wrapper.append(input);}input.required=required;if(help)wrapper.append(node('small',help));return wrapper;}
function editor(title,description,fields,action,submit='Save'){$('editor-title').textContent=title;$('editor-description').textContent=description;$('editor-fields').replaceChildren(...fields);$('editor-submit').textContent=submit;$('editor-error').hidden=true;editorAction=action;$('editor').showModal();const input=$('editor-fields').querySelector('input,select');if(input)input.focus();}
function closeEditor(){if(!busy)$('editor').close();}
$('close-editor').onclick=closeEditor;$('cancel-editor').onclick=closeEditor;
$('editor').addEventListener('cancel',e=>{if(busy)e.preventDefault();});
$('editor-form').onsubmit=e=>{e.preventDefault();task(async()=>{await editorAction(new FormData(e.target));$('editor').close();e.target.reset();});};
$('pair-form').onsubmit=e=>{e.preventDefault();task(async()=>{const result=await api('/api/pair',{code:$('code').value.trim()});token=result.token;sessionStorage.setItem('madari-tv-token',token);$('code').value='';await refresh();});};
$('refresh').onclick=()=>task(refresh);
$('disconnect').onclick=()=>task(async()=>{await api('/api/logout',{});token='';sessionStorage.removeItem('madari-tv-token');showPairing();});
function selectProfile(profile){editor(`Open ${profile.name}`,profile.kids?"Enter the guardian's PIN to manage this kids profile.":'Enter the profile PIN, or leave it empty if no PIN was set.',[field(profile.kids?'Guardian PIN':'Profile PIN','pin',{type:'password'})],async form=>command('select_profile',{id:profile.id,pin:form.get('pin')}),'Open settings');}
function render(){
 $('pairing').hidden=true;$('dashboard').hidden=false;$('profiles').replaceChildren();
 for(const p of model.profiles){const b=button('',()=>selectProfile(p),'profile-card'+(model.selected?.id===p.id?' selected':''));b.append(node('span',p.name.slice(0,1).toUpperCase(),'avatar'));const text=node('span',p.name);text.append(node('small',p.kids?'Kids · Guardian PIN':p.pin_protected?'PIN protected':'Regular profile'));b.append(text);$('profiles').append(b);}
 $('kids-notice').hidden=!model.active_kids;if(model.active_kids)$('kids-notice').textContent=`Kids mode is active for ${model.active_kids.name}. To manage another profile, leave kids mode on the TV using the guardian PIN.`;
 $('managed').hidden=!model.selected;$('profile-caption').textContent=model.selected?`Managing ${model.selected.name} · Watching on the TV is unchanged.`:'Choose a profile to manage.';
 if(!model.selected)return;
 renderAddons();const prefs=model.snapshot.playback_preferences;for(const [key,value]of Object.entries(prefs)){const input=$('preferences-form').elements.namedItem(key);if(!input)continue;if(input.type==='checkbox')input.checked=value;else input.value=Array.isArray(value)?value.join(', '):value;}
 $('profile-form').elements.namedItem('name').value=model.selected.name;$('profile-form').elements.namedItem('pin').value='';$('profile-form').elements.namedItem('pin').disabled=model.selected.kids;
}
function renderAddons(){const list=$('addon-list');list.replaceChildren();const addons=model.snapshot.addons;if(!addons.length){list.append(node('div','No addons yet. Paste a configured manifest URL to get started.','empty'));return;}
 addons.forEach((addon,index)=>{const card=node('article',null,'addon'),top=node('div',null,'addon-top'),title=node('div');title.append(node('h3',addon.manifest.name),node('small',`Version ${addon.manifest.version} · ${addon.manifest.types.join(', ')}`));top.append(title,node('span',addon.enabled?'Enabled':'Disabled','badge'+(addon.enabled?'':' off')));card.append(top);if(addon.manifest.description)card.append(node('p',addon.manifest.description));const actions=node('div',null,'actions');
 actions.append(button(addon.enabled?'Disable':'Enable',()=>task(()=>command('enable',{id:addon.installation_id,enabled:!addon.enabled}))));
 if(index>0)actions.append(button('↑ Move up',()=>moveAddon(index,-1)));if(index<addons.length-1)actions.append(button('↓ Move down',()=>moveAddon(index,1)));
 actions.append(button('Reconfigure',()=>installEditor(addon)),button('Share',()=>shareEditor(addon)),button('Remove',()=>editor('Remove addon?',`Remove ${addon.manifest.name} from ${model.selected.name}? Other linked profiles keep their addon.`,[],()=>command('remove_addon',{id:addon.installation_id}),'Remove'),'danger'));
 card.append(actions);list.append(card);
 });}
function moveAddon(index,delta){task(async()=>{const ids=model.snapshot.addons.map(a=>a.installation_id);[ids[index],ids[index+delta]]=[ids[index+delta],ids[index]];await command('reorder',ids);});}
function installEditor(addon=null){editor(addon?'Reconfigure addon':'Install addon',addon?'Paste the new configured manifest URL. Shared profiles also receive this configuration change.':'Paste the complete configured manifest URL from your addon provider.',[
 field('Manifest URL','url',{required:true,type:'url',help:'For example: https://your-addon.example/manifest.json'}),field('Allow access to a local-network addon','allow_local',{type:'checkbox',value:addon?.allow_local||false})
 ],form=>command(addon?'configure':'install',{id:addon?.installation_id,url:form.get('url').trim(),allow_local:form.has('allow_local')}),addon?'Save configuration':'Install');}
$('install').onclick=()=>installEditor();
function shareEditor(addon){const targets=model.profiles.filter(p=>p.id!==model.selected.id);if(!targets.length){message('Create another profile first.');return;}editor('Share addon','The installation is linked. Configuration is shared; order, enabled status, library and progress stay independent.',[
 field('Share with','target_id',{options:targets.map(p=>[p.id,p.name+(p.kids?' (Kids)':'')])}),field('Target profile or guardian PIN','pin',{type:'password',help:'Leave empty when sharing to your own kids profile or a profile without a PIN.'})
 ],form=>command('share',{id:addon.installation_id,target_id:form.get('target_id'),pin:form.get('pin')}),'Share addon');}
$('new-profile').onclick=()=>editor('Create a profile',model.profiles.length?'An unlocked regular profile can create profiles. Kids profiles need a PIN-protected guardian.':'Create your first regular profile.',[
 field('Name','name',{required:true}),field('PIN (optional, 4–8 digits)','pin',{type:'password',pattern:'[0-9]{4,8}'}),...(model.profiles.length?[field('Kids profile','kids',{type:'checkbox'})]:[])
],form=>command('create_profile',{name:form.get('name'),pin:form.get('pin'),kids:form.has('kids')}),'Create profile');
$('preferences-form').onsubmit=e=>{e.preventDefault();task(async()=>{const form=new FormData(e.target),preferences={...model.snapshot.playback_preferences};for(const key of ['audio_languages','subtitle_languages'])preferences[key]=String(form.get(key)).split(',').map(s=>s.trim()).filter(Boolean);for(const key of ['subtitle_sdh','subtitle_forced','audio_description','audio_commentary'])preferences[key]=form.get(key);preferences.subtitles_enabled=form.has('subtitles_enabled');await command('preferences',preferences);});};
$('profile-form').onsubmit=e=>{e.preventDefault();task(()=>command('update_profile',{name:e.target.elements.namedItem('name').value,pin:e.target.elements.namedItem('pin').value}));};
$('lock-profile').onclick=()=>task(()=>command('lock_profile'));
document.querySelectorAll('[data-tab]').forEach(button=>button.onclick=()=>{document.querySelectorAll('[data-tab]').forEach(b=>b.classList.toggle('active',b===button));document.querySelectorAll('.tab-panel').forEach(p=>p.hidden=p.id!==button.dataset.tab);});
if(token)task(refresh);else showPairing();
