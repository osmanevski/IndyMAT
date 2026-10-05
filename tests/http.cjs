const fs=require('fs'),path=require('path'),assert=require('assert');
// Standalone publish regressions: no server, launch token, or user files.
function checkPublishOffline(){
 const {spawnSync}=require('child_process'),tmp=fs.mkdtempSync(path.join(require('os').tmpdir(),'indymat-publish-'));
 const quote=value=>"'"+value.replaceAll("'","''")+"'";
 const checks=[];
 const check=(input,expected)=>checks.push({input,expected});
 const resources={link:['href'],img:['src','srcset'],iframe:['src'],embed:['src'],object:['data','codebase'],source:['src','srcset'],video:['src','poster'],audio:['src'],track:['src'],input:['src']};
 for(const [tag,attributes] of Object.entries(resources))for(const attribute of attributes){
  for(const value of ['"https://example.invalid/x>y"',"'http://example.invalid/x>y'",'https://example.invalid/x','//example.invalid/x']){
   const name=tag.toUpperCase(),attr=attribute.toUpperCase();
   check(`<${name} alt="1 > 0" ${attr}=${value}>`,`<${name} alt="1 > 0" >`);
  }
 }
 for(const value of ['"https://example.invalid/x>y"',"'http://example.invalid/x>y'",'https://example.invalid/x','//example.invalid/x']){
  check(`before<ScRiPt alt='1 > 0' SrC=${value}>alert('x')</sCrIpT>after`,'beforeafter');
 }
 check('<script>MathJax={};</script>','');
 check('<img srcset="local.png 1x, //example.invalid/x.png 2x">','<img >');
 for(const value of ['https://example.invalid/x','http://example.invalid/x','//example.invalid/x']){
  for(const argument of [value,`"${value}>y"`,`'${value}>y'`]){
   check(`<StYlE>.a{background:UrL(${argument});color:red}</sTyLe>`,'<StYlE>.a{background:url();color:red}</sTyLe>');
   check(`<style>@IMPORT url(${argument}) screen and (min-width: 1px);.a{color:red}</style>`,'<style>.a{color:red}</style>');
   const delimiter=argument.includes('"')?"'":'"';
   check(`<div title="1 > 0" STYLE=${delimiter}background:UrL(${argument});color:red${delimiter}>text</div>`,`<div title="1 > 0" STYLE=${delimiter}background:url();color:red${delimiter}>text</div>`);
   check(`<div STYLE=${delimiter}@import url(${argument}); color:red${delimiter}>text</div>`,`<div STYLE=${delimiter} color:red${delimiter}>text</div>`);
  }
  for(const delimiter of ['"',"'"]){
   check(`<style>@import ${delimiter}${value}${delimiter} print;.a{color:red}</style>`,'<style>.a{color:red}</style>');
   const outer=delimiter==='"'?"'":'"';
   check(`<div STYLE=${outer}@import ${delimiter}${value}${delimiter} print; color:red${outer}>text</div>`,`<div STYLE=${outer} color:red${outer}>text</div>`);
  }
 }
 const unchanged=[
  '<pre>url(https://example.invalid/code) @import "//example.invalid/print";</pre>',
  '<pre class="oct-code-output">https://example.invalid/output url(//example.invalid/output)</pre>',
  '<code>&lt;img alt="1 &gt; 0" src="https://example.invalid/code"&gt;</code>',
  '<!-- source: <img src="https://example.invalid/comment"> url(https://example.invalid/comment) -->',
  '<a href="https://example.invalid/link">https://example.invalid/text</a>',
  '<img alt="url(https://example.invalid/description)" src="plot.png">',
  '<style>/* url(https://example.invalid/comment) */.a{content:"url(https://example.invalid/text)";background:url(local.png)}</style>',
  '<div style="background:url(local.png)">local image</div>',
  '<textarea><img src="https://example.invalid/text"></textarea>',
  '<title>url(https://example.invalid/title)</title>'
 ];
 for(const html of unchanged)check(html,html);
 try{
  const root=path.resolve(__dirname,'..');
  const runner=["warning('off','Octave:shadowed-function');",`addpath(${quote(path.join(root,'octave'))});`,`addpath(${quote(path.join(root,'octave','compat'))});`,`setappdata(0,'__mf_folder__',${quote(tmp)});`];
  checks.forEach(({input,expected},i)=>{
   const file=path.join(tmp,`case-${i}`);fs.writeFileSync(file+'.in',input);fs.writeFileSync(file+'.out',expected);
   runner.push(`assert(strcmp(__mf_publish_html__(fileread(${quote(file+'.in')})),fileread(${quote(file+'.out')})), 'publish resource case ${i}');`);
  });
  runner.push(`fprintf('PUBLISH RESOURCE PASS: ${checks.length} cases\\n');`);
  fs.mkdirSync(path.join(tmp,'sources'));
  for(const previous of ['absent','false','true']){
   const caller=path.join(tmp,'caller-'+previous),source=path.join(tmp,'sources',`probe_${previous}.m`);fs.mkdirSync(caller);
   fs.writeFileSync(source,`%% Publish cleanup $x^2$\n% <html>\n% <img alt="1 > 0" src="https://example.invalid/x.png">\n% </html>\nrmdir(${quote(caller)});\nclc;\ndisp('url(https://example.invalid/output)');\n`);
   runner.push(previous==='absent'?"if isappdata(0,'__mf_publishing__'),rmappdata(0,'__mf_publishing__');endif":`setappdata(0,'__mf_publishing__',${previous});`);
   runner.push(`cd(${quote(caller)});`,`setappdata(0,'__mf_job__','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');`,
    `failed=false; try; __mf_publish__(${quote(source)}); catch err; failed=true; assert(!isempty(strfind(err.message,${quote(caller)})),err.message); end_try_catch; assert(failed);`,
    previous==='absent'?"assert(!isappdata(0,'__mf_publishing__'));":`assert(isappdata(0,'__mf_publishing__')); assert(isequal(getappdata(0,'__mf_publishing__'),${previous}));`,
    `assert(${previous==='true'?'isempty':'!isempty'}(strfind(evalc('clc;'),'__MF_CLC_')));`,
    `cd(${quote(tmp)});`,
    `html=fileread(${quote(path.join(tmp,'publish',`probe_${previous}.html`))});`,
    "assert(!isempty(strfind(html,'<img alt=\"1 > 0\" >'))); assert(!isempty(strfind(html,'<pre class=\"oct-code-output\">url(https://example.invalid/output)')));",
    "assert(isempty(strfind(html,'__MF_'))); assert(isempty(strfind(html,'__mf_'))); assert(!isempty(strfind(html,'$x^2$')));"
   );
  }
  runner.push("rmappdata(0,'__mf_publishing__'); disp('PUBLISH CLEANUP PASS: absent/false/true flag restored after deleted cwd; live clc restored');");
  const success=path.join(tmp,'sources','publish_success.m');
  fs.writeFileSync(success,"%% Successful publish $x^2$\nclc;\ndisp(42);\n");
  runner.push(`before=pwd(); report=__mf_publish__(${quote(success)}); assert(strcmp(pwd(),before)); assert(!isappdata(0,'__mf_publishing__')); assert(exist(report,'file')==0); html=fileread(${quote(path.join(tmp,'publish','publish_success.html'))}); assert(!isempty(strfind(html,'42'))); assert(isempty(strfind(html,'<script'))); assert(isempty(strfind(html,'__MF_'))); assert(!isempty(strfind(evalc('clc;'),'__MF_CLC_'))); disp('PUBLISH SUCCESS PASS: report privately staged (not installed), cwd/flag restored, live clc preserved');`);
  const file=path.join(tmp,'checks.m');fs.writeFileSync(file,runner.join('\n')+'\n');
  const result=spawnSync('octave-cli',['--quiet','--no-history',file],{encoding:'utf8',timeout:60000,cwd:tmp});
  assert.ifError(result.error);assert.equal(result.status,0,result.stdout+'\n'+result.stderr);process.stdout.write(result.stdout);
 }finally{fs.rmSync(tmp,{recursive:true,force:true});}
}
(async()=>{
 if(process.argv.includes('--publish-only')){checkPublishOffline();return;}
 const {url}=JSON.parse(fs.readFileSync('.matlab-free/launch.json'));const [base,token]=url.split('#');
 const request=(p,opts={})=>fetch(base+p,{...opts,headers:{'X-MF-Language':'tr','X-MF-Token':token,...opts.headers}});
 const waitIdle=async()=>{for(let i=0;i<200;i++){let state=await (await request('api/state')).json();if(state.status==='idle')return state;await new Promise(resolve=>setTimeout(resolve,50));}throw new Error('Octave job timeout');};
 const waitPaused=async()=>{for(let i=0;i<200;i++){let state=await (await request('api/state')).json();if(state.status==='paused'&&state.debug?.ready)return state;await new Promise(resolve=>setTimeout(resolve,50));}throw new Error('Octave debug timeout');};
 assert.equal((await fetch(base+'api/state')).status,403);
 assert.equal((await request('api/state',{headers:{Origin:'https://evil.example'}})).status,403);
 const badHost=await new Promise((resolve,reject)=>{require('http').get(base+'api/state',{headers:{Host:'evil.example','X-MF-Language':'tr','X-MF-Token':token}},r=>{r.resume();resolve(r.statusCode)}).on('error',reject)});assert.equal(badHost,403);
 assert.equal((await request('api/file?path='+encodeURIComponent('../README.md'))).status,403);
 assert.equal((await request('api/file?path='+encodeURIComponent('/etc/passwd'))).status,403);
 const name='http-test-'+Date.now()+'.m';
 let r=await request('api/file',{method:'POST',body:JSON.stringify({path:name,content:'v=1;',hash:'absent'})});assert.equal(r.status,200);let saved=await r.json();
 assert.equal((await request('api/file',{method:'POST',body:JSON.stringify({path:name,content:'v=2;',hash:'wrong'})})).status,409);
 fs.writeFileSync('workspace/'+name,'external=3;');
 assert.equal((await request('api/file',{method:'POST',body:JSON.stringify({path:name,content:'v=4;',hash:saved.hash})})).status,409);
 let loaded=await (await request('api/file?path='+name)).json();assert.equal(loaded.content,'external=3;');
 assert.equal((await request('api/file?path='+name)).status,200);
 const lintJob=(await (await request('api/state')).json()).job;
 let lint=await request('api/lint',{method:'POST',body:JSON.stringify({code:'x=1;'})});assert.equal(lint.status,200);assert.deepStrictEqual((await lint.json()).issues,[]);
 lint=await request('api/lint',{method:'POST',body:JSON.stringify({code:'x = ;'})});assert.equal(lint.status,200);assert.equal((await lint.json()).issues[0].line,1);
 assert.equal((await request('api/lint',{method:'POST',body:JSON.stringify({code:'x',path:'not-accepted.m'})})).status,400);
 assert.equal((await request('api/lint',{method:'POST',body:JSON.stringify({code:'x'.repeat(500001)})})).status,400);
 const state=await (await request('api/state')).json();assert.equal(state.version,'11.3.0');assert.equal(state.job,lintJob,'lint changed the persistent Octave session');
 const helpFile='http_help_probe.m';fs.writeFileSync('workspace/'+helpFile,'function y=http_help_probe(x)\n% HTTP_HELP_PROBE Kullanıcı yardım testi.\ny=x;\nendfunction\n');
 let doc=await (await request('api/help?name=fft')).json();assert.equal(doc.origin,'core');assert(doc.text.includes('Fourier'));
 doc=await (await request('api/help?name=butter')).json();assert.equal(doc.origin,'package');assert(doc.origin_label.includes('signal'));
 doc=await (await request('api/help?name=http_help_probe')).json();assert.equal(doc.origin,'user');assert(doc.text.includes('Kullanıcı yardım testi'));
 doc=await (await request('api/help?name=http_missing_help')).json();assert.equal(doc.found,false);assert.equal(doc.message,'http_missing_help bulunamadı.');
 assert.equal((await request('api/help?name=../fft')).status,400);assert.equal((await (await request('api/state')).json()).job,lintJob,'help changed the persistent Octave session');
 let packageList=await (await request('api/packages')).json();assert(packageList.packages.some(p=>p.name==='signal'&&p.loaded));
 r=await request('api/execute',{method:'POST',body:JSON.stringify({mode:'code',code:'http_package_kept=909;'})});assert.equal(r.status,202);await waitIdle();
 r=await request('api/package-session',{method:'POST',body:JSON.stringify({action:'unload',name:'signal'})});assert.equal(r.status,202);let packageState=await waitIdle();assert(!packageState.packages.find(p=>p.name==='signal').loaded);
 r=await request('api/package-session',{method:'POST',body:JSON.stringify({action:'load',name:'signal'})});assert.equal(r.status,202);packageState=await waitIdle();assert(packageState.packages.find(p=>p.name==='signal').loaded);assert(packageState.variables.some(v=>v.name==='http_package_kept'));
 assert.equal((await request('api/package-admin',{method:'POST',body:JSON.stringify({action:'install',source:'forge',name:'signal'})})).status,400);
 fs.unlinkSync('workspace/'+helpFile);
 const hidden='http_history_hidden_'+Date.now()+'=1;';r=await request('api/execute',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({mode:'code',code:hidden})});assert.equal(r.status,202);await waitIdle();let history=await (await request('api/history')).json();assert(!history.includes(hidden));
 const historyCode='http_history_a_'+Date.now()+'=1;\nhttp_history_b=2;';r=await request('api/execute',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({mode:'code',code:historyCode,history:true})});assert.equal(r.status,202);await waitIdle();history=await (await request('api/history')).json();assert.equal(history.at(-1),historyCode);
 const debugName='http_debug_test.m',debugPath=path.resolve('workspace',debugName);fs.writeFileSync(debugPath,'http_debug_a=2;\nhttp_debug_b=http_debug_a+5;\nhttp_debug_done=http_debug_b*2;\n');
 r=await request('api/breakpoint',{method:'POST',body:JSON.stringify({path:debugPath,line:2,enabled:true})});assert.equal(r.status,202);await waitIdle();r=await request('api/execute',{method:'POST',body:JSON.stringify({mode:'file',argument:debugPath})});assert.equal(r.status,202);let paused=await waitPaused();assert.equal(paused.debug.line,2);assert(paused.variables.some(v=>v.name==='http_debug_a'));r=await request('api/debug',{method:'POST',body:JSON.stringify({command:'step'})});assert.equal(r.status,200);paused=await waitPaused();assert.equal(paused.debug.line,3);r=await request('api/debug',{method:'POST',body:JSON.stringify({command:'continue'})});assert.equal(r.status,200);let debugDone=await waitIdle();assert(debugDone.variables.some(v=>v.name==='http_debug_done'&&v.preview==='14'));r=await request('api/breakpoint',{method:'POST',body:JSON.stringify({path:debugPath,line:2,enabled:false})});assert.equal(r.status,202);await waitIdle();fs.unlinkSync(debugPath);
 const folder=path.join(state.workspace,'http-folder-'+Date.now());fs.mkdirSync(folder);for(let i=0;i<1001;i++)fs.writeFileSync(path.join(folder,`${String(i).padStart(4,'0')}.m`),i?'':'http_folder_answer=654;');fs.symlinkSync('/etc',path.join(folder,'escape'));
 assert.equal((await request('api/folder',{method:'POST',body:JSON.stringify({path:'/etc'})})).status,403);
 assert.equal((await request('api/file?path='+encodeURIComponent(path.join(folder,'escape/passwd')))).status,403);
 r=await request('api/folder',{method:'POST',body:JSON.stringify({path:folder})});assert.equal(r.status,202);let listing=await (await request('api/files')).json();assert.equal(listing.entries.length,1000);assert.equal(listing.truncated,true);assert.equal(listing.current,folder);await waitIdle();r=await request('api/execute',{method:'POST',body:JSON.stringify({mode:'file',argument:path.join(folder,'0000.m')})});assert.equal(r.status,202);let ran=await waitIdle();assert(ran.variables.some(v=>v.name==='http_folder_answer'));
 r=await request('api/folder',{method:'POST',body:JSON.stringify({path:state.workspace})});assert.equal(r.status,202);await waitIdle();r=await request('api/execute',{method:'POST',body:JSON.stringify({mode:'code',code:`cd('${folder}')`})});assert.equal(r.status,202);let followed=await waitIdle();assert.equal(followed.current,folder);r=await request('api/folder',{method:'POST',body:JSON.stringify({path:state.workspace})});assert.equal(r.status,202);await waitIdle();fs.rmSync(folder,{recursive:true});
 const profileHelper='http_profile_helper_'+Date.now(),profilePath=path.join(state.workspace,'http_profile.m'),helperPath=path.join(state.workspace,profileHelper+'.m');fs.writeFileSync(profilePath,`for k=1:3; http_profile_value=${profileHelper}(k); end\n`);fs.writeFileSync(helperPath,`function y=${profileHelper}(x)\n y=cos(x);\nend\n`);
 r=await request('api/profile',{method:'POST',body:JSON.stringify({path:profilePath})});assert.equal(r.status,202);let profiled=await waitIdle();assert.equal(profiled.kind,'profile');assert(!profiled.error,profiled.error);let helperRow=profiled.profile.find(row=>row.name===profileHelper);assert.equal(helperRow.calls,3);assert.equal(helperRow.file,helperPath);assert.equal(helperRow.line,1);
 assert.equal((await request('api/profile',{method:'POST',body:JSON.stringify({path:'../outside.m'})})).status,403);
 checkPublishOffline();
 const publishPath=path.join(state.workspace,'http_publish.m');fs.writeFileSync(publishPath,String.raw`%% HTTP yayın $x^2$
%
% $$e^{i\pi}+1=0$$
clc;
http_published_value=6*7;
disp(http_published_value);
`);r=await request('api/publish',{method:'POST',body:JSON.stringify({path:publishPath})});assert.equal(r.status,202);let publishStart=await r.json();assert.equal(typeof publishStart.render,'string');let published=await waitIdle();assert.equal(published.kind,'publish');assert(!published.error,published.error);assert.equal(published.publish.staged,false);assert.equal(published.publish.images,0);assert(fs.existsSync(published.publish.path));
 let fallback=fs.readFileSync(published.publish.path,'utf8');assert(fallback.includes('$x^2$'));assert(fallback.includes(String.raw`$$e^{i\pi}+1=0$$`));assert(!/<script\b/i.test(fallback),'un-rendered fallback contains a script');assert(!/<(?:script|link|img)\b[^>]*(?:src|href)\s*=\s*["']?https?:\/\//i.test(fallback),'un-rendered fallback contains a remote resource');assert(/<head><meta http-equiv="Content-Security-Policy"/.test(fallback),'disk fallback has no first-child CSP');assert(!fs.existsSync(path.join('.matlab-free','jobs',publishStart.job,'publish')),'private raw staging was not cleaned');
 assert.equal((await fetch(base+'api/published-render?job='+publishStart.job+'&render='+publishStart.render)).status,403);let sourceResponse=await request('api/published-render?job='+publishStart.job+'&render='+encodeURIComponent(publishStart.render));assert.equal(sourceResponse.status,200);assert((await sourceResponse.text()).includes('$x^2$'));assert.equal((await request('api/published-render',{method:'POST',body:JSON.stringify({job:publishStart.job,render:publishStart.render,path:'/tmp/other.html',html:'<svg></svg>'})})).status,400);assert.equal((await request('api/published-render',{method:'POST',body:JSON.stringify({job:publishStart.job,render:publishStart.render,html:'<svg></svg><script></script>'})})).status,400);assert.equal(fs.readFileSync(published.publish.path,'utf8'),fallback,'rejected render changed the fallback report');
 assert.equal((await fetch(base+'api/published?path='+encodeURIComponent(published.publish.path))).status,403);let reportResponse=await request('api/published?path='+encodeURIComponent(published.publish.path));assert.equal(reportResponse.status,200);let reportCsp=reportResponse.headers.get('content-security-policy');assert(reportCsp.includes('sandbox'));assert(!reportCsp.includes('script-src'));let report=await reportResponse.text();assert(report.includes('http-equiv="Content-Security-Policy"'));assert(report.replaceAll('&#x27;',"'").includes("default-src 'none'"));assert(report.includes('http_published_value=6*7'));assert(report.includes('42'));assert(report.includes('$x^2$'));assert(!/<(?:script|link|img)\b[^>]*(?:src|href)\s*=\s*["']?https?:\/\//i.test(report),'published report contains a remote resource');assert(!/__MF_|__mf_/.test(report),'published report contains an internal protocol marker');
 const unsafePath=path.join(state.workspace,'http_publish_unsafe.m');fs.writeFileSync(unsafePath,'%% Unsafe raw HTML\n% <html>\n% <img src="plot.png" onerror="alert(1)">\n% </html>\ndisp(1);\n');
 r=await request('api/publish',{method:'POST',body:JSON.stringify({path:unsafePath})});assert.equal(r.status,202);const unsafeStart=await r.json(),unsafeResult=await waitIdle();assert(unsafeResult.error&&unsafeResult.error.includes('güvenli'),unsafeResult.error);assert(!unsafeResult.publish);assert(!fs.existsSync(path.join(state.workspace,'html','http_publish_unsafe.html')),'unsafe fallback installed');assert(!fs.existsSync(path.join('.matlab-free','jobs',unsafeStart.job,'publish')),'unsafe private staging retained');fs.unlinkSync(unsafePath);
 fs.unlinkSync(profilePath);fs.unlinkSync(helperPath);fs.unlinkSync(publishPath);fs.unlinkSync(published.publish.path);try{fs.rmdirSync(path.dirname(published.publish.path));}catch{}
 fs.unlinkSync('workspace/'+name);
 console.log('HTTP PASS: request guards, home boundary, symlink escape, bounded listing, folder/file run, debugger, profile, publish, isolated help, package list/session actions, explicit install confirmation, pwd follow, save conflicts, parse-only lint, command-only history.');
})().catch(e=>{console.error(e);process.exit(1)});
