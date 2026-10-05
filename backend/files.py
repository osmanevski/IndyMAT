from backend.i18n import tr
from pathlib import Path
import errno, hashlib, os, tempfile, shutil, stat, secrets
from contextlib import contextmanager

class Workspace:
    TEXT={'.m','.txt','.csv','.json','.md','.tsv'}
    ALLOWED=TEXT|{'.mat'}
    IGNORED={'.git','node_modules','__pycache__','Library','.Trash','.cache','.npm','.local'}
    def __init__(self,path,allowed_root=None):
        self.root=Path(path).resolve();self.root.mkdir(parents=True,exist_ok=True)
        self.home=Path(allowed_root).resolve() if allowed_root is not None else Path.home().resolve()
        if allowed_root is not None and not self.root.is_relative_to(self.home):raise PermissionError(tr('The working folder is outside the allowed root.'))
        self.roots=(self.home,) if self.root.is_relative_to(self.home) else (self.home,self.root)
        self.current=self.root
    def _inside(self,p):return any(p.is_relative_to(root) for root in self.roots)
    def _boundary(self,p):return next(root for root in self.roots if p.is_relative_to(root))
    def path(self,name):
        if not isinstance(name,str) or not name or any(c in name for c in ('\x00','\n','\r')):raise ValueError(tr('Invalid file path.'))
        raw=Path(name).expanduser()
        if '..' in raw.parts:raise PermissionError(tr('The file path cannot traverse a parent folder.'))
        p=(raw if raw.is_absolute() else self.current/raw).resolve()
        if any(p.is_relative_to(root) for root in getattr(self,'workspace_private_roots',())):raise PermissionError(tr('Private MAT job files cannot be accessed.'))
        if not self._inside(p) or p==self.current:raise PermissionError(tr('The file is outside the allowed folder.'))
        return p
    def folder(self,name):
        if not isinstance(name,(str,os.PathLike)):raise ValueError(tr('Invalid folder path.'))
        name=os.fspath(name)
        if not name or any(c in name for c in ('\x00','\n','\r')):raise ValueError(tr('Invalid folder path.'))
        raw=Path(name).expanduser();p=(raw if raw.is_absolute() else self.current/raw).resolve()
        if any(p.is_relative_to(root) for root in getattr(self,'workspace_private_roots',())):raise PermissionError(tr('The private MAT job folder cannot be accessed.'))
        if not self._inside(p):raise PermissionError(tr('The folder is outside the home directory.'))
        if not p.is_dir():raise FileNotFoundError(tr('Folder not found.'))
        return p
    def set_current(self,name):self.current=self.folder(name);return self.current
    def follow(self,name):
        try:self.set_current(name);return True
        except (OSError,ValueError,PermissionError):return False
    def info(self):
        boundary=self._boundary(self.current)
        return {'workspace':str(self.root),'root':str(boundary),'current':str(self.current)}
    def tree(self,limit=1000):
        out=[];truncated=False;seen=0
        try:entries=os.scandir(self.current)
        except OSError:raise PermissionError(tr('Could not read the folder.'))
        with entries:
            for entry in entries:
                seen+=1
                if len(out)>=limit or seen>limit*2:truncated=True;break
                if entry.name.startswith('.') or entry.name in self.IGNORED:continue
                try:
                    p=Path(entry.path).resolve()
                    if not self._inside(p):continue
                    directory=entry.is_dir(follow_symlinks=True)
                    if not directory and not entry.is_file(follow_symlinks=True):continue
                    out.append({'name':entry.name,'path':str(p),'directory':directory,'size':0 if directory else entry.stat(follow_symlinks=True).st_size,'editable':not directory and p.suffix.lower() in self.TEXT})
                except OSError:continue
        out=sorted(out,key=lambda item:(not item['directory'],item['name'].casefold()))
        return {**self.info(),'entries':out,'truncated':truncated,'limit':limit}
    def read(self,name):
        p=self.path(name)
        if p.suffix.lower() not in self.TEXT:raise ValueError(tr('This file cannot be opened in the Editor. Use load().'))
        if p.stat().st_size>2_000_000:raise ValueError(tr('The Editor limit is 2 MB. Read the file from the Command Window.'))
        data=p.read_bytes()
        try:content=data.decode('utf-8-sig')
        except UnicodeDecodeError:raise ValueError(tr('The file is not UTF-8. Save it as UTF-8 in its editor and import it again.'))
        return {'path':name,'content':content,'hash':hashlib.sha256(data).hexdigest()}
    def save(self,name,content,expected=None,binary=False):
        p=self.path(name)
        if p.suffix.lower() not in self.ALLOWED:raise ValueError(tr('Supported files: .m, .mat, .csv, .tsv, .txt, .json, .md'))
        data=content if binary else content.replace('\r\n','\n').encode('utf-8')
        return self._atomic_write(p,data,expected,name)

    def operate(self,request,session_folder=None):
        from backend.file_operations import FileOperations
        return FileOperations(self).operate(request,session_folder)

    @contextmanager
    def _report_parent(self,name,create=False):
        """Walk from / using directory fds, never following a symlink.

        Unlike editor paths, report capabilities must not resolve aliases anew.
        The returned directory identity chain pins every parent component.
        """
        if not isinstance(name,str) or any(c in name for c in '\x00\r\n'):raise ValueError(tr('Invalid publish path.'))
        p=Path(name).expanduser()
        if not p.is_absolute():p=self.current/p
        if '..' in p.parts or not self._inside(p):raise PermissionError(tr('The publish path is outside the allowed folder.'))
        if p.suffix.lower()!='.html' or p.parent.name!='html':raise ValueError(tr('Invalid published report.'))
        fd=os.open('/',os.O_RDONLY|os.O_DIRECTORY);chain=[]
        try:
            for index,part in enumerate(p.parts[1:-1]):
                if create and index==len(p.parts)-3:
                    try:os.mkdir(part,0o755,dir_fd=fd)
                    except FileExistsError:pass
                try:next_fd=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=fd)
                except OSError as exc:raise PermissionError(tr('The publish path contains a link or a changed folder.')) from exc
                os.close(fd);fd=next_fd;s=os.fstat(fd);chain.append((s.st_dev,s.st_ino))
            yield p,fd,tuple(chain)
        finally:os.close(fd)

    @staticmethod
    def _report_read_at(fd,name,absent=False):
        try:source=os.open(name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=fd)
        except FileNotFoundError:
            if absent:return None,None
            raise FileExistsError(tr('The published report was deleted. Publish it again.'))
        except OSError as exc:raise PermissionError(tr('The published file cannot be a symbolic link.')) from exc
        with os.fdopen(source,'rb') as stream:
            before=os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode):raise PermissionError(tr('The published file must be a regular file.'))
            if before.st_size>20_000_000:raise ValueError(tr('The published report is too large.'))
            data=stream.read(20_000_001);after=os.fstat(stream.fileno())
        if len(data)>20_000_000:raise ValueError(tr('The published report is too large.'))
        if (before.st_size,before.st_mtime_ns,before.st_ctime_ns)!=(after.st_size,after.st_mtime_ns,after.st_ctime_ns):
            raise FileExistsError(tr('The published report changed while being read.'))
        return data,(before.st_dev,before.st_ino)

    def read_published(self,name,binding=None,*,absent=False,create=False):
        with self._report_parent(name,create) as (p,fd,chain):
            data,identity=self._report_read_at(fd,p.name,absent)
            current={'path':str(p),'parents':chain,'identity':identity,
                     'hash':hashlib.sha256(data).hexdigest() if data is not None else None}
            if binding is not None and current!=binding:raise FileExistsError(tr('The published file or folder changed. Publish it again.'))
            return data,current

    def replace_published(self,name,content,expected,binding):
        """Capability-only write: compare bytes AND inode/parents, no symlinks.

        App holds kernel -> publish -> file locks across this operation. All
        filesystem operations below are directory-relative. A raced displaced
        file is retained as recovery, never followed or silently overwritten.
        """
        if not isinstance(content,str):raise ValueError(tr('The published report must be text.'))
        data=content.encode('utf-8')
        if len(data)>20_000_000:raise ValueError(tr('The published report is too large.'))
        with self._report_parent(name) as (p,fd,chain):
            if str(p)!=binding['path'] or chain!=binding['parents']:raise FileExistsError(tr('The publish folder changed.'))
            old,identity=self._report_read_at(fd,p.name,absent=binding['identity'] is None)
            digest=hashlib.sha256(old).hexdigest() if old is not None else None
            if identity!=binding['identity'] or digest!=expected or digest!=binding['hash']:
                raise FileExistsError(tr('The published report changed externally. Publish it again.'))
            mode=stat.S_IMODE(os.stat(p.name,dir_fd=fd,follow_symlinks=False).st_mode) if identity else 0o644
            temporary='.mf-report-'+secrets.token_hex(16)
            recovery='.mf-report-recovery-'+secrets.token_hex(16)
            moved=False;installed=False
            out=os.open(temporary,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o644,dir_fd=fd)
            try:
                with os.fdopen(out,'wb') as stream:
                    os.fchmod(stream.fileno(),mode)
                    stream.write(data);stream.flush();os.fsync(stream.fileno())
                # Re-walk immediately before mutation: parent renames/replacements
                # must not redirect a capability into a different directory.
                with self._report_parent(name) as (_,check_fd,check_chain):
                    if check_chain!=chain:raise FileExistsError(tr('The publish folder changed.'))
                if identity is not None:
                    os.rename(p.name,recovery,src_dir_fd=fd,dst_dir_fd=fd);moved=True
                    displaced,displaced_id=self._report_read_at(fd,recovery)
                    if displaced_id!=identity or hashlib.sha256(displaced).hexdigest()!=expected:
                        raise FileExistsError(tr('The published report changed while saving; recovery file: {path}', path=str(p.parent / recovery)))
                # link is an atomic, no-clobber install; it cannot follow a
                # newly-created final symlink (unlike opening the target).
                try:os.link(temporary,p.name,src_dir_fd=fd,dst_dir_fd=fd,follow_symlinks=False)
                except FileExistsError:
                    note=tr('; recovery file: {path}', path=str(p.parent / recovery)) if moved else ''
                    raise FileExistsError(tr('The published file was recreated while saving{note}', note=note))
                installed=True
                written,new_id=self._report_read_at(fd,p.name)
                if written!=data:raise FileExistsError(tr('The published report changed while saving.'))
                with self._report_parent(name) as (_,check_fd,check_chain):
                    if check_chain!=chain:raise FileExistsError(tr('The publish folder changed while saving.'))
                if moved:
                    preserved,preserved_id=self._report_read_at(fd,recovery)
                    if preserved_id!=identity or hashlib.sha256(preserved).hexdigest()!=expected:
                        raise FileExistsError(tr('The published report changed while saving; recovery file: {path}', path=str(p.parent / recovery)))
                    os.unlink(recovery,dir_fd=fd);moved=False
                return {'path':str(p),'hash':hashlib.sha256(data).hexdigest(),
                        'binding':{'path':str(p),'parents':chain,'identity':new_id,'hash':hashlib.sha256(data).hexdigest()}}
            finally:
                if moved and not installed:
                    try:os.link(recovery,p.name,src_dir_fd=fd,dst_dir_fd=fd,follow_symlinks=False)
                    except FileExistsError:pass
                os.unlink(temporary,dir_fd=fd)
    def _atomic_write(self,p,data,expected,name):
        existed=p.exists()
        if existed:
            try:current=hashlib.sha256(p.read_bytes()).hexdigest()
            except FileNotFoundError:raise FileExistsError(tr('The file was deleted externally. Save with a different name.'))
            if expected != current: raise FileExistsError(tr('The file changed externally or already exists. Reopen it or save with a different name.'))
        elif expected not in (None,'absent'):raise FileExistsError(tr('The file was deleted externally. Save with a different name.'))
        if len(data)>20_000_000:raise ValueError(tr('The file limit is 20 MB.'))
        p.parent.mkdir(parents=True,exist_ok=True)
        fd,tmp=tempfile.mkstemp(dir=p.parent,prefix='.mf-')
        recovery=None;moved=False
        try:
            with os.fdopen(fd,'wb') as f:f.write(data)
            if existed:
                try:shutil.copymode(p,tmp)
                except FileNotFoundError:raise FileExistsError(tr('The file was deleted externally. Save with a different name.'))
                rfd,recovery=tempfile.mkstemp(dir=p.parent,prefix=f'.mf-recovery-{p.name}-');os.close(rfd)
                try:os.replace(p,recovery);moved=True
                except FileNotFoundError:raise FileExistsError(tr('The file was deleted externally. Save with a different name.'))
                location=str(Path(recovery).relative_to(self._boundary(p)))
                message=tr('The file changed externally while saving. The preserved version is in the recovery file "{path}". Reopen it or save with a different name.', path=location)
                if hashlib.sha256(Path(recovery).read_bytes()).hexdigest()!=current:
                    try:os.link(recovery,p)
                    except FileExistsError:pass
                    raise FileExistsError(message)
            else:os.chmod(tmp,0o644)
            try:os.link(tmp,p)
            except FileExistsError:
                if recovery:raise FileExistsError(message)
                raise FileExistsError(tr('The file was created externally while saving. Reopen it or save with a different name.'))
            try:written=hashlib.sha256(Path(p).read_bytes()).hexdigest()
            except FileNotFoundError:
                if recovery:raise FileExistsError(message)
                raise FileExistsError(tr('The file was deleted externally while saving. Try saving again.'))
            if written!=hashlib.sha256(data).hexdigest():
                if recovery:raise FileExistsError(message)
                raise FileExistsError(tr('The file changed externally while saving. Reopen it or save with a different name.'))
            if recovery:
                if hashlib.sha256(Path(recovery).read_bytes()).hexdigest()!=current:raise FileExistsError(message)
                os.unlink(recovery);recovery=None
        finally:
            if os.path.exists(tmp):os.unlink(tmp)
            if recovery and not moved and os.path.exists(recovery):os.unlink(recovery)
        return {'path':name,'hash':hashlib.sha256(data).hexdigest()}
