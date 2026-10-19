import {promises as fs} from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const [platform,target]=process.argv.slice(2);
if(!['windows-x64','macos-arm64','macos-x64'].includes(platform)||!target)throw Error('platform and target required');
const root=process.cwd(),build=path.join(root,'target',target,'release'),out=path.join(root,'artifacts');
await fs.mkdir(out,{recursive:true});
async function nonempty(file){const stat=await fs.stat(file);if(!stat.isFile()||stat.size===0)throw Error('Missing/empty release file: '+file);}
async function walk(dir){const entries=await fs.readdir(dir,{withFileTypes:true});return (await Promise.all(entries.map(e=>e.isDirectory()?walk(path.join(dir,e.name)):path.join(dir,e.name)))).flat();}
const files=await walk(path.join(build,'bundle'));
const extensions=platform==='windows-x64'?['.exe','.msi']:['.dmg'];
for(const ext of extensions){const candidates=files.filter(f=>f.endsWith(ext));if(candidates.length!==1)throw Error('Expected exactly one installer '+ext);await nonempty(candidates[0]);await fs.copyFile(candidates[0],path.join(out,'HoriPlay-'+platform+(ext==='.exe'?'-setup':'')+ext));}
const portable=path.join(root,'.cache','portable-'+platform);
await fs.mkdir(portable,{recursive:true});
for(const name of ['horiplay','horiplay-signaling']){const file=name+(platform.startsWith('windows')?'.exe':'');await nonempty(path.join(build,file));await fs.copyFile(path.join(build,file),path.join(portable,file));}
const library=platform.startsWith('windows')?'SDL3.dll':'libSDL3.0.dylib';
await nonempty(path.join(root,'runtime',library));await fs.copyFile(path.join(root,'runtime',library),path.join(portable,library));
for(const name of ['LICENSE','README.en.md','THIRD-PARTY.md'])await fs.copyFile(path.join(root,name),path.join(portable,name));
if(platform.startsWith('windows')){
  const destination=path.join(out,'HoriPlay-tools-'+platform+'.zip');
  execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',"$ErrorActionPreference='Stop'; Compress-Archive -Path (Join-Path $env:HP_PORTABLE '*') -DestinationPath $env:HP_ARCHIVE -Force"],{env:{...process.env,HP_PORTABLE:portable,HP_ARCHIVE:destination},stdio:'inherit'});
}else{execFileSync('tar',['-czf',path.join(out,'HoriPlay-tools-'+platform+'.tar.gz'),'-C',portable,'.'],{stdio:'inherit'});}
const checks=[];
for(const name of await fs.readdir(out)){const file=path.join(out,name);await nonempty(file);checks.push({name,bytes:(await fs.stat(file)).size,sha256:createHash('sha256').update(await fs.readFile(file)).digest('hex')});}
await fs.writeFile(path.join(out,'SHA256SUMS-'+platform+'.txt'),checks.map(v=>v.sha256+'  '+v.name).join('\n')+'\n');
const sha=process.env.GITHUB_SHA||execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
if(!/^[a-f0-9]{40}$/.test(sha))throw Error('Full source commit required');
await fs.writeFile(path.join(out,'BUILD-'+platform+'.json'),JSON.stringify({version:'0.1.0-alpha.1',source_commit:sha,platform,target,profile:'release',rust:execFileSync('rustc',['-V'],{encoding:'utf8'}).trim(),built_utc:new Date().toISOString(),application_signed:false,macos_notarized:false,scope:'Controller input alpha. Not complete remote play. No physical-controller/game acceptance claimed.',files:checks},null,2)+'\n');

