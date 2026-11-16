//go:build linux && amd64

// One-shot native owner candidate, uncompiled and not workflow-enabled.
// This image consumes live objects from the reviewed same-job bootstrap.
// Root-owned FDs bind those objects; they are not independent issuer authentication against host root.
package main

import (
 "bytes"
 "bufio"
 "context"
 "crypto/rand"
 "crypto/sha256"
 "encoding/binary"
 "encoding/hex"
 "encoding/json"
 "errors"
 "fmt"
 "io"
 "os"
 "os/exec"
 "os/signal"
 "path/filepath"
 "runtime"
 "sort"
 "strconv"
 "strings"
 "sync/atomic"
 "syscall"
 "time"
 "unsafe"
)
const fixedCLI="/opt/hostedtoolcache/CodeQL/2.27.4/x64/codeql/codeql"
const maxBundle int64=512<<20
const maxDB int64=8<<30
const prSetNoNewPrivs=38
type config struct { Nonce,Root string; UID uint32; Deadline int64; ParentPIDNamespace uint64 }
type childResult struct { Nonce string; CLIPID,RootExit,Reaped int; AllReaped,Cancelled,LateWriter bool; Error string }
type ownerReceipt struct {
 Version int `json:"version"`
 Nonce string
 FileHeld bool
 OwnerPID,NamespacePID,CLIPID int
 PrivateUID uint32
 RootExit,Reaped int
 AllReaped,LateWriter,CleanupVerified bool
 SourceDevice uint64 `json:",string"`
 SourceInode uint64 `json:",string"`
 OutputDevice uint64 `json:",string"`
 OutputInode uint64 `json:",string"`
 Bytes int
 SHA256,Acquisition,CopyGraphSHA256 string
}
func fail(s string)error{return errors.New(s)}
func stat(f *os.File)(*syscall.Stat_t,error){var s syscall.Stat_t;e:=syscall.Fstat(int(f.Fd()),&s);return &s,e}
func same(a,b *syscall.Stat_t)bool{return a.Dev==b.Dev&&a.Ino==b.Ino}
func regular(s *syscall.Stat_t)bool{return s.Mode&syscall.S_IFMT==syscall.S_IFREG}
func anchor(f *os.File)string{return fmt.Sprintf("/proc/self/fd/%d",f.Fd())}
func nofollow(p string,flags int)(*os.File,error){n,e:=syscall.Open(p,flags|syscall.O_CLOEXEC|syscall.O_NOFOLLOW,0);if e!=nil{return nil,e};return os.NewFile(uintptr(n),p),nil}
func namespaceID()(uint64,error){i,e:=os.Stat("/proc/self/ns/pid");if e!=nil{return 0,e};s,ok:=i.Sys().(*syscall.Stat_t);if !ok{return 0,fail("namespace_stat")};return s.Ino,nil}
func rootCapabilities()error{
 if os.Geteuid()!=0{return fail("native_root_unavailable")};b,e:=os.ReadFile("/proc/self/status");if e!=nil{return e};var mask uint64;found:=false
 for _,line:=range strings.Split(string(b),"\n"){if strings.HasPrefix(line,"CapEff:"){mask,e=strconv.ParseUint(strings.TrimSpace(strings.TrimPrefix(line,"CapEff:")),16,64);if e!=nil{return e};found=true}}
 need:=uint64(1<<0|1<<1|1<<6|1<<7|1<<18|1<<21) // CHOWN,DAC_OVERRIDE,SETGID,SETUID,SYS_CHROOT,SYS_ADMIN
 if !found||mask&need!=need{return fail("native_capabilities_unavailable")};return nil
}
func selfImage()(string,*os.File,error){f,e:=os.Open("/proc/self/exe");if e!=nil{return "",nil,e};s,e:=stat(f);if e!=nil||!regular(s)||s.Uid!=0||s.Mode&022!=0{f.Close();return "",nil,fail("native_image_unqualified")};return "/proc/self/exe",f,nil}
func sealControlFD(fd int)error{_,_,e:=syscall.Syscall(syscall.SYS_FCNTL,uintptr(fd),syscall.F_SETFD,syscall.FD_CLOEXEC);if e!=0{return e};flags,_,e:=syscall.Syscall(syscall.SYS_FCNTL,uintptr(fd),syscall.F_GETFD,0);if e!=0{return e};if flags&syscall.FD_CLOEXEC==0{return fail("native_control_fd_unsealed")};return nil}
func unusedUID()(uint32,error){
 const census="/proc/self/fd/15";var procFS syscall.Statfs_t;if syscall.Fstatfs(15,&procFS)!=nil||procFS.Type!=0x9fa0{return 0,fail("host_proc_capability_unavailable")};dirs,e:=os.ReadDir(census);if e!=nil{return 0,e};used:=map[uint32]bool{}
 for _,d:=range dirs{if _,e:=strconv.Atoi(d.Name());e!=nil{continue};b,e:=os.ReadFile(filepath.Join(census,d.Name(),"status"));if os.IsNotExist(e){continue};if e!=nil{return 0,fail("uid_census_incomplete")};for _,line:=range strings.Split(string(b),"\n"){if strings.HasPrefix(line,"Uid:"){for _,v:=range strings.Fields(strings.TrimPrefix(line,"Uid:")){n,e:=strconv.ParseUint(v,10,32);if e!=nil{return 0,e};used[uint32(n)]=true}}}}
 var b [4]byte;for i:=0;i<16;i++{if _,e:=rand.Read(b[:]);e!=nil{return 0,e};uid:=uint32(100000)+binary.LittleEndian.Uint32(b[:])%1000000000;if !used[uid]{return uid,nil}};return 0,fail("private_uid_unavailable")
}
type graphMember struct{Name string `json:"name"`;Bytes int64 `json:"bytes"`;SHA256 string `json:"sha256"`}
type copyBudget struct{ ctx context.Context; deadline time.Time; files,dirs int; bytes int64; records []graphMember }
func copyGraphHash(records []graphMember)(string,error){sort.Slice(records,func(i,j int)bool{return records[i].Name<records[j].Name});h:=sha256.New();for _,r:=range records{var size [12]byte;binary.LittleEndian.PutUint32(size[:4],uint32(len([]byte(r.Name))));binary.LittleEndian.PutUint64(size[4:],uint64(r.Bytes));h.Write(size[:]);h.Write([]byte(r.Name));digest,e:=hex.DecodeString(r.SHA256);if e!=nil{return "",e};h.Write(digest)};return hex.EncodeToString(h.Sum(nil)),nil}
func copyDB(root *os.File,dest,relative string,b *copyBudget,depth int)error{
 b.dirs++;if depth>64||b.dirs>4096||(b.ctx!=nil&&b.ctx.Err()!=nil)||time.Now().After(b.deadline){return fail("database_copy_budget")};before,e:=stat(root);if e!=nil||before.Mode&syscall.S_IFMT!=syscall.S_IFDIR{return fail("database_directory")};entries,e:=os.ReadDir(anchor(root));if e!=nil{return e};sort.Slice(entries,func(i,j int)bool{return entries[i].Name()<entries[j].Name()})
 for _,entry:=range entries{
  name:=entry.Name();if name==""||name=="."||name==".."||strings.ContainsAny(name,"/\\\x00"){return fail("database_name")};source:=filepath.Join(anchor(root),name);info,e:=os.Lstat(source);if e!=nil||info.Mode()&os.ModeSymlink!=0{return fail("database_link")};target:=filepath.Join(dest,name)
  if info.IsDir(){if e=os.Mkdir(target,0700);e!=nil{return e};child,e:=nofollow(source,syscall.O_RDONLY|syscall.O_DIRECTORY);if e!=nil{return e};e=copyDB(child,target,filepath.ToSlash(filepath.Join(relative,name)),b,depth+1);ce:=child.Close();if e!=nil{return e};if ce!=nil{return ce};continue}
  if !info.Mode().IsRegular()||b.files>=20000{return fail("database_member")};in,e:=nofollow(source,syscall.O_RDONLY|syscall.O_NONBLOCK);if e!=nil{return e};s,e:=stat(in);if e!=nil||!regular(s)||s.Size<0||s.Size>maxDB{in.Close();return fail("database_file")};out,e:=os.OpenFile(target,os.O_WRONLY|os.O_CREATE|os.O_EXCL,0600);if e!=nil{in.Close();return e}
  var copied int64;digest:=sha256.New();buf:=make([]byte,64<<10)
  for copied<s.Size{if time.Now().After(b.deadline){e=fail("database_copy_deadline");break};n,re:=in.Read(buf[:min(int64(len(buf)),s.Size-copied)]);if n>0{nw,we:=out.Write(buf[:n]);if nw>0{digest.Write(buf[:nw])};copied+=int64(nw);if we!=nil||nw!=n{e=fail("database_copy_write");break}};if re!=nil{if re==io.EOF&&copied==s.Size{break};e=re;break};if n==0{e=io.ErrNoProgress;break}}
  after,se:=stat(in);if e==nil&&(se!=nil||!same(s,after)||s.Size!=after.Size||s.Mtim!=after.Mtim||s.Ctim!=after.Ctim||copied!=s.Size){e=fail("database_changed_during_copy")};ci,co:=in.Close(),out.Close();if e!=nil{return e};if ci!=nil{return ci};if co!=nil{return co};copiedFile,ce:=nofollow(target,syscall.O_RDONLY|syscall.O_NONBLOCK);if ce!=nil{return ce};checked:=sha256.New();count,ce:=io.Copy(checked,io.LimitReader(copiedFile,s.Size+1));closeErr:=copiedFile.Close();if ce!=nil||closeErr!=nil||count!=s.Size||!bytes.Equal(checked.Sum(nil),digest.Sum(nil)){return fail("database_copy_bytes")};b.records=append(b.records,graphMember{filepath.ToSlash(filepath.Join(relative,name)),copied,hex.EncodeToString(digest.Sum(nil))});b.files++;b.bytes+=copied;if b.bytes>maxDB{return fail("database_total")}
 }
 after,e:=stat(root);if e!=nil||!same(before,after)||before.Mtim!=after.Mtim||before.Ctim!=after.Ctim{return fail("database_directory_changed")};return nil
}
func assignPrivate(tree string,uid uint32)error{return filepath.WalkDir(tree,func(p string,d os.DirEntry,e error)error{if e!=nil{return e};i,e:=os.Lstat(p);if e!=nil||i.Mode()&os.ModeSymlink!=0{return fail("private_tree_link")};return os.Chown(p,int(uid),int(uid))})}
func bindReadOnly(source,target string,dir bool)error{
 if dir{if e:=os.MkdirAll(target,0755);e!=nil{return e}}else{f,e:=os.OpenFile(target,os.O_CREATE|os.O_EXCL|os.O_WRONLY,0600);if e!=nil{return e};if e=f.Close();e!=nil{return e}}
 if e:=syscall.Mount(source,target,"",syscall.MS_BIND|syscall.MS_REC,"");e!=nil{return e};return syscall.Mount("",target,"",syscall.MS_BIND|syscall.MS_REMOUNT|syscall.MS_RDONLY|syscall.MS_NOSUID,"")
}
func setupRoot(c config)error{
 if e:=syscall.Mount("","/","",syscall.MS_REC|syscall.MS_PRIVATE,"");e!=nil{return e}
 for _,source:=range []string{"/usr","/bin","/lib","/lib64","/opt/hostedtoolcache/CodeQL/2.27.4/x64/codeql"}{i,e:=os.Stat(source);if e!=nil{return e};if e=bindReadOnly(source,filepath.Join(c.Root,source),i.IsDir());e!=nil{return e}}
 for _,source:=range []string{"/etc/ld.so.cache","/etc/nsswitch.conf","/etc/hosts"}{if _,e:=os.Stat(source);e!=nil{return e};target:=filepath.Join(c.Root,source);if e:=os.MkdirAll(filepath.Dir(target),0755);e!=nil{return e};if e:=bindReadOnly(source,target,false);e!=nil{return e}}
 if e:=os.MkdirAll(filepath.Join(c.Root,"proc"),0555);e!=nil{return e};if e:=syscall.Mount("proc",filepath.Join(c.Root,"proc"),"proc",syscall.MS_NOSUID|syscall.MS_NODEV|syscall.MS_NOEXEC,"");e!=nil{return e}
 if e:=os.MkdirAll(filepath.Join(c.Root,"dev"),0755);e!=nil{return e};for _,source:=range []string{"/dev/null","/dev/urandom"}{target:=filepath.Join(c.Root,source);f,e:=os.OpenFile(target,os.O_CREATE|os.O_EXCL|os.O_WRONLY,0600);if e!=nil{return e};if e=f.Close();e!=nil{return e};if e:=syscall.Mount(source,target,"",syscall.MS_BIND,"");e!=nil{return e}};return nil
}
func killNamespace()error{if os.Getpid()!=1{return fail("not_owned_namespace_init")};return syscall.Kill(-1,syscall.SIGKILL)}

// Only namespace PID1 may invoke this kernel reaper. It is not a JSON grant.
func reapWriterSet(r *childResult,rootPID int){
 if os.Getpid()!=1||rootPID<2{r.Error="not_owned_namespace_init";return}
 rootSeen:=false
 for{
  options:=0;if rootSeen{options=syscall.WNOHANG}
  var status syscall.WaitStatus;got,e:=syscall.Wait4(-1,&status,options,nil)
  if e==syscall.EINTR{continue};if e==syscall.ECHILD{r.AllReaped=true;return};if e!=nil{r.Error="wait4";return}
  if got==0{r.LateWriter=true;if e:=killNamespace();e!=nil{r.Error="namespace_stop";return};rootSeen=false;continue}
  r.Reaped++;if got==rootPID&&r.RootExit==-1{if status.Exited(){r.RootExit=status.ExitStatus()}else{r.RootExit=128+int(status.Signal())};rootSeen=true}
 }
}


// The writer inherits these filters through fork and exec. Only this locked
// OS thread starts the CLI; the privileged namespace reaper is not filtered.
func writerFilters()[]syscall.SockFilter{
 const ld=0x20;const jeq=0x15;const jge=0x35;const ret=0x06;const deny=0x00050000|uint32(syscall.EPERM);const allow=0x7fff0000;const kill=0x80000000
 p:=[]syscall.SockFilter{{Code:ld,K:4},{Code:jeq,Jt:1,K:0xc000003e},{Code:ret,K:kill},{Code:ld,K:0},{Code:jge,Jt:0,Jf:1,K:512},{Code:ret,K:kill}}
 // connect/sendto/sendmsg/sendmmsg/io_uring prevent external FD export;
 // ptrace/process_vm_writev and namespace changes cannot escape the writer set.
 for _,n:=range []uint32{42,44,46,307,101,311,308,272,425,426,427}{p=append(p,syscall.SockFilter{Code:jeq,Jt:0,Jf:1,K:n},syscall.SockFilter{Code:ret,K:deny})}
 return append(p,syscall.SockFilter{Code:ret,K:allow})
}
func installWriterFilter()error{p:=writerFilters();prog:=syscall.SockFprog{Len:uint16(len(p)),Filter:&p[0]};_,_,e:=syscall.RawSyscall(syscall.SYS_PRCTL,22,2,uintptr(unsafe.Pointer(&prog)));runtime.KeepAlive(p);if e!=0{return e};return nil}

func internalInit()int{
 if os.Getpid()!=1||os.Geteuid()!=0{return 70};control,report:=os.NewFile(3,"owner_control"),os.NewFile(4,"owner_result");if control==nil||report==nil{return 70};defer control.Close();defer report.Close();if sealControlFD(3)!=nil||sealControlFD(4)!=nil{return 70};loadedImage:=os.NewFile(5,"loaded_owner_image");if loadedImage==nil||loadedImage.Close()!=nil{return 70}
 var c config;dec:=json.NewDecoder(io.LimitReader(control,16385));if dec.Decode(&c)!=nil||dec.Decode(new(any))!=io.EOF||len(c.Nonce)!=32||c.UID<100000{return 70};ns,e:=namespaceID();if e!=nil||ns==c.ParentPIDNamespace{return 70}
 r:=childResult{Nonce:c.Nonce,RootExit:-1};finish:=func(code int)int{b,e:=json.Marshal(r);if e!=nil||len(b)>1024{return 70};b=append(b,byte(10));n,e:=report.Write(b);if e!=nil||n!=len(b){return 70};return code};if e:=setupRoot(c);e!=nil{r.Error="isolation_setup";return finish(70)}
 var cancelled atomic.Bool;signals:=make(chan os.Signal,1);signal.Notify(signals,syscall.SIGTERM,syscall.SIGINT);defer signal.Stop(signals);timer:=time.NewTimer(time.Until(time.Unix(0,c.Deadline)));defer timer.Stop();done:=make(chan struct{});go func(){select{case <-signals:cancelled.Store(true);killNamespace();case <-timer.C:cancelled.Store(true);killNamespace();case <-done:}}();defer close(done)
 if cancelled.Load()||time.Now().After(time.Unix(0,c.Deadline)){r.Cancelled=true;return finish(70)}
 runtime.LockOSThread();_,_,errno:=syscall.RawSyscall6(syscall.SYS_PRCTL,prSetNoNewPrivs,1,0,0,0,0);if errno!=0{runtime.UnlockOSThread();r.Error="no_new_privs";return finish(70)}
 args:=[]string{fixedCLI,"database","bundle","/db","--output=/out/bundle.zip","--name=rust","--cache-cleanup=clear","--verbosity=errors"};if _,e:=os.Stat(filepath.Join(c.Root,"db/base-database-oids.json"));e==nil{args=append(args,"--include","base-database-oids.json")}else if !os.IsNotExist(e){runtime.UnlockOSThread();r.Error="database_include";return finish(70)}
 entries,e:=os.ReadDir(filepath.Join(c.Root,"db"));if e!=nil{runtime.UnlockOSThread();r.Error="database_projection";return finish(70)};for _,entry:=range entries{name:=entry.Name();if name=="src.zip"||name=="codeql-database.yml"||name=="base-database-oids.json"||name=="db-rust"{continue};if entry.IsDir()&&(name=="log"||name=="logs"||name=="results"||name=="trap"||name=="temp"||name=="src"){continue};args=append(args,"--include",name)}
 null,e:=os.Open("/dev/null");if e!=nil{runtime.UnlockOSThread();r.Error="stdin";return finish(70)};defer null.Close()
 if e:=installWriterFilter();e!=nil{runtime.UnlockOSThread();r.Error="writer_filter";return finish(70)}
 pid,e:=syscall.ForkExec(fixedCLI,args,&syscall.ProcAttr{Dir:"/",Env:[]string{"PATH=/usr/bin:/bin","HOME=/home","TMPDIR=/tmp","LANG=C.UTF-8"},Files:[]uintptr{null.Fd(),os.Stderr.Fd(),os.Stderr.Fd()},Sys:&syscall.SysProcAttr{Chroot:c.Root,Credential:&syscall.Credential{Uid:c.UID,Gid:c.UID,Groups:[]uint32{}},Setpgid:true}});runtime.UnlockOSThread();if e!=nil{r.Error="cli_start";return finish(70)};r.CLIPID=pid
 if cancelled.Load(){killNamespace()}
 reapWriterSet(&r,pid)
 r.Cancelled=cancelled.Load();if r.Cancelled||r.LateWriter||!r.AllReaped||r.RootExit!=0||r.Error!=""{return finish(70)};return finish(0)
}

// childOwner retains the actual exec.Cmd/process object. Stop never reuses a
// numeric PID after Wait has been observed; every wait and escalation is bounded.
type childOwner struct { cmd *exec.Cmd; done <-chan error; observed bool; result error }
func startOwned(cmd *exec.Cmd)(*childOwner,error){if e:=cmd.Start();e!=nil{return nil,e};done:=make(chan error,1);go func(){done<-cmd.Wait()}();return &childOwner{cmd:cmd,done:done},nil}
func(o *childOwner)poll()bool{if o.observed{return true};select{case e:=<-o.done:o.observed=true;o.result=e;return true;default:return false}}
func(o *childOwner)wait(ctx context.Context)error{if o.poll(){return o.result};select{case e:=<-o.done:o.observed=true;o.result=e;return e;case <-ctx.Done():return ctx.Err()}}
func(o *childOwner)stop()error{
 if o.poll(){return o.result};o.cmd.Process.Signal(syscall.SIGTERM)
 grace,cancel:=context.WithTimeout(context.Background(),2*time.Second);e:=o.wait(grace);cancel();if o.observed{return e}
 o.cmd.Process.Kill();drain,finish:=context.WithTimeout(context.Background(),2*time.Second);defer finish();e=o.wait(drain);if !o.observed{return fail("native_reap_unverified")};return e
}
// This parses data from the private root-init pipe, not a caller authority.
// Only run(), after the actual owned child Wait, may use it for acquisition.
func checkedChildResult(raw []byte,nonce string)(childResult,error){
 var r childResult
 if len(raw)==0||len(raw)>16384||len(nonce)!=32{return r,fail("native_report")}
 d:=json.NewDecoder(bytes.NewReader(raw));d.DisallowUnknownFields()
 if d.Decode(&r)!=nil||d.Decode(new(any))!=io.EOF||r.Nonce!=nonce{return r,fail("native_report")}
 if !r.AllReaped||r.RootExit!=0||r.Cancelled||r.LateWriter||r.Error!=""||r.CLIPID<2||r.Reaped<1{return r,fail("native_writer_generation_failed")};return r,nil
}


type launchControl struct{ Nonce string; RemainingMillis int64 }
type outputAck struct{ Nonce,Kind,SHA256 string }
func readControlLine(ctx context.Context,r *bufio.Reader)([]byte,error){
 type result struct{body []byte;err error};ch:=make(chan result,1)
 go func(){b,e:=r.ReadSlice('\n');if len(b)>4096{e=fail("native_control_size")};ch<-result{append([]byte(nil),b...),e}}()
 select{case v:=<-ch:return v.body,v.err;case <-ctx.Done():return nil,ctx.Err()}
}

func run()error{
 if len(os.Args)!=1{return fail("native_arguments")}
 // Inspect FD3 before opening anything: an absent inherited capability must not
 // accidentally become a later opened image/status descriptor.
 var inherited syscall.Stat_t
 if e:=syscall.Fstat(3,&inherited);e!=nil||inherited.Mode&syscall.S_IFMT!=syscall.S_IFDIR{return fail("database_capability_unavailable")}
 db:=os.NewFile(3,"held_database");if db==nil{return fail("database_capability_unavailable")};defer db.Close();if e:=sealControlFD(3);e!=nil{return e}
 if e:=rootCapabilities();e!=nil{return e};if e:=sealControlFD(15);e!=nil{return e};_,image,e:=selfImage();if e!=nil{return e};defer image.Close();var inheritedImage syscall.Stat_t;if syscall.Fstat(5,&inheritedImage)!=nil{return fail("native_image_capability")};loaded,e:=stat(image);if e!=nil||!same(loaded,&inheritedImage){return fail("native_image_object_changed")};ds,e:=stat(db);if e!=nil||!same(&inherited,ds){return fail("database_capability_changed")}
 // FD3 comes from the code-owned qualified bootstrap, never an argv path/hash.
 var stageFDStat syscall.Stat_t
 if e=syscall.Fstat(10,&stageFDStat);e!=nil||stageFDStat.Mode&syscall.S_IFMT!=syscall.S_IFDIR||stageFDStat.Uid!=0||stageFDStat.Mode&0777!=0700{return fail("native_job_stage_capability_unavailable")}
 stageCap:=os.NewFile(10,"job_stage");if stageCap==nil{return fail("native_job_stage_capability_unavailable")};defer stageCap.Close();if e=sealControlFD(10);e!=nil{return e}
 control:=bufio.NewReaderSize(os.Stdin,4096);initial,cancelInitial:=context.WithTimeout(context.Background(),5*time.Second);first,re:=readControlLine(initial,control);cancelInitial();if re!=nil{return fail("native_launch_control")}
 var launch launchControl;decoder:=json.NewDecoder(bytes.NewReader(first));decoder.DisallowUnknownFields()
 if decoder.Decode(&launch)!=nil||decoder.Decode(new(any))!=io.EOF||len(launch.Nonce)!=32||launch.RemainingMillis<1||launch.RemainingMillis>240000{return fail("native_launch_control")}
 overallDeadline:=time.Now().Add(time.Duration(launch.RemainingMillis)*time.Millisecond)
 operationCtx,cancelOperation:=context.WithDeadline(context.Background(),overallDeadline);defer cancelOperation()
 type ackRead struct{value outputAck;err error};ackChannel:=make(chan ackRead,1)
 go func(){line,e:=readControlLine(operationCtx,control);var ack outputAck;if e==nil{d:=json.NewDecoder(bytes.NewReader(line));d.DisallowUnknownFields();if d.Decode(&ack)!=nil||d.Decode(new(any))!=io.EOF||ack.Nonce!=launch.Nonce||(ack.Kind!="accept"&&ack.Kind!="reject"){e=fail("native_output_ack_invalid")}};ackChannel<-ackRead{ack,e};if e!=nil||ack.Kind=="reject"{cancelOperation()}}()
 rawStage,e:=os.MkdirTemp(anchor(stageCap),"native-owner-");if e!=nil{return e};fmt.Fprintf(os.Stderr,"native_owner_stage_raw=%q\n",rawStage)
 stage,e:=filepath.EvalSymlinks(rawStage);if e!=nil{fmt.Fprintf(os.Stderr,"native_owner_cleanup_unverified=%q reason=stage_resolution\n",rawStage);return e};fmt.Fprintf(os.Stderr,"native_owner_stage=%q\n",stage)
 stageInfo,e:=os.Lstat(stage);if e!=nil{fmt.Fprintf(os.Stderr,"native_owner_cleanup_unverified=%q reason=initial_stage_stat\n",stage);return e};stageIdentity:=stageInfo.Sys().(*syscall.Stat_t)
 started,reaped,cleaned:=false,false,false
 cleanup:=func()error{now,e:=os.Lstat(stage);if e!=nil||!same(stageIdentity,now.Sys().(*syscall.Stat_t)){return fail("native_cleanup_stage_identity")};if started&&!reaped{return fail("native_cleanup_writers_unverified")};if e:=os.RemoveAll(stage);e!=nil{return e};if _,e:=os.Lstat(stage);!os.IsNotExist(e){return fail("native_cleanup_not_absent")};cleaned=true;return nil}
 defer func(){var cleanupErr error;if !cleaned{cleanupErr=cleanup()};record:=struct{Nonce string;Verified bool;WritersReaped bool;Stage string;Error string}{Nonce:launch.Nonce,Verified:cleaned,WritersReaped:!started||reaped,Stage:stage};if cleanupErr!=nil{record.Error=cleanupErr.Error()};b,_:=json.Marshal(record);fmt.Fprintf(os.Stderr,"native_owner_cleanup=%s\n",b)}()
 if e=os.Chmod(stage,0700);e!=nil{return e};root:=filepath.Join(stage,"root");if e=os.Mkdir(root,0755);e!=nil{return e};for _,dir:=range []string{"db","out","tmp","home"}{if e=os.Mkdir(filepath.Join(root,dir),0700);e!=nil{return e}}
 b:=copyBudget{ctx:operationCtx,deadline:overallDeadline};if e=copyDB(db,filepath.Join(root,"db"),"",&b,0);e!=nil{return e};for _,required:=range []string{"src.zip","codeql-database.yml","db-rust"}{if _,e=os.Stat(filepath.Join(root,"db",required));e!=nil{return fail("database_components")}}
 copyGraph,e:=copyGraphHash(b.records);if e!=nil{return e}
 uid,e:=unusedUID();if e!=nil{return e};for _,dir:=range []string{"db","out","tmp","home"}{if e=assignPrivate(filepath.Join(root,dir),uid);e!=nil{return e}};ns,e:=namespaceID();if e!=nil{return e};var nonce [16]byte;if _,e=rand.Read(nonce[:]);e!=nil{return e};cliDeadline:=time.Now().Add(60*time.Second);if overallDeadline.Before(cliDeadline){cliDeadline=overallDeadline};c:=config{Nonce:hex.EncodeToString(nonce[:]),Root:root,UID:uid,Deadline:cliDeadline.UnixNano(),ParentPIDNamespace:ns}
 controlBytes,e:=json.Marshal(c);if e!=nil||len(controlBytes)>1024{return fail("native_control_size")};controlBytes=append(controlBytes,byte(10))
 if operationCtx.Err()!=nil||time.Now().After(overallDeadline){return fail("native_deadline_or_cancelled")}
 inputR,inputW,e:=os.Pipe();if e!=nil{return e};defer inputR.Close();reportR,reportW,e:=os.Pipe();if e!=nil{inputW.Close();return e};defer reportR.Close()
 cmd:=exec.Command("/proc/self/fd/5","--internal-owner-init");cmd.ExtraFiles=[]*os.File{inputR,reportW,image};cmd.Stdout=os.Stderr;cmd.Stderr=os.Stderr;cmd.Env=[]string{"PATH=/usr/bin:/bin"};cmd.SysProcAttr=&syscall.SysProcAttr{Cloneflags:syscall.CLONE_NEWNS|syscall.CLONE_NEWPID|syscall.CLONE_NEWNET|syscall.CLONE_NEWIPC,Pdeathsig:syscall.SIGKILL}
 runtime.LockOSThread();defer runtime.UnlockOSThread()
 owned,e:=startOwned(cmd);if e!=nil{inputW.Close();reportW.Close();return fail("native_namespaces_unavailable")};started=true;inputR.Close();reportW.Close()
 written,e:=inputW.Write(controlBytes);if e!=nil||written!=len(controlBytes){inputW.Close();owned.stop();return fail("native_control_write_failed")};inputW.Close()
 ctx,cancel:=context.WithDeadline(operationCtx,cliDeadline);defer cancel();e=owned.wait(ctx)
 if !owned.observed{owned.stop();return fail("native_deadline_or_reap_unverified")}
 raw,re:=io.ReadAll(io.LimitReader(reportR,16385));if re!=nil||len(raw)>16384{return fail("native_report")}
 result,validation:=checkedChildResult(raw,c.Nonce);reaped=result.AllReaped
 if e!=nil||validation!=nil{return fail("native_writer_generation_failed")};if operationCtx.Err()!=nil||time.Now().After(overallDeadline){return fail("native_deadline_or_cancelled")}
 out:=filepath.Join(root,"out","bundle.zip");if e=os.Chown(filepath.Join(root,"out"),0,0);e!=nil{return e};if e=os.Chmod(filepath.Join(root,"out"),0500);e!=nil{return e};names,e:=os.ReadDir(filepath.Join(root,"out"));if e!=nil||len(names)!=1||names[0].Name()!="bundle.zip"{return fail("native_output_set")}
 f,e:=nofollow(out,syscall.O_RDONLY|syscall.O_NONBLOCK);if e!=nil{return e};defer f.Close();s,e:=stat(f);if e!=nil||!regular(s)||s.Nlink!=1||s.Uid!=uid||s.Size<1||s.Size>maxBundle{return fail("native_output_object")};if e=f.Chown(0,0);e!=nil{return e};if e=f.Chmod(0400);e!=nil{return e};before,e:=stat(f);if e!=nil{return e}
 body,e:=io.ReadAll(io.LimitReader(f,maxBundle+1));if e!=nil||int64(len(body))!=before.Size{return fail("native_output_bytes")};after,e:=stat(f);if e!=nil||!same(before,after)||before.Size!=after.Size||before.Mtim!=after.Mtim||before.Ctim!=after.Ctim{return fail("native_output_changed")};if operationCtx.Err()!=nil||time.Now().After(overallDeadline){return fail("native_deadline_or_cancelled")};
 digest:=sha256.Sum256(body);receipt:=ownerReceipt{Version:1,Nonce:launch.Nonce,FileHeld:true,OwnerPID:os.Getpid(),NamespacePID:cmd.Process.Pid,CLIPID:result.CLIPID,PrivateUID:uid,RootExit:result.RootExit,Reaped:result.Reaped,AllReaped:result.AllReaped,LateWriter:result.LateWriter,CleanupVerified:false,SourceDevice:uint64(ds.Dev),SourceInode:ds.Ino,OutputDevice:uint64(before.Dev),OutputInode:before.Ino,Bytes:len(body),SHA256:hex.EncodeToString(digest[:]),CopyGraphSHA256:copyGraph,Acquisition:"root-owned-outer-stage;private-uid;new-pid-mount-net-ipc-namespace;chroot;wait4-ECHILD;frozen-held-fd"}
 header,e:=json.Marshal(receipt);if e!=nil||len(header)>16384{return fail("native_output_header")};var length [4]byte;binary.BigEndian.PutUint32(length[:],uint32(len(header)))
 if n,e:=os.Stdout.Write(length[:]);e!=nil||n!=len(length){return fail("native_output_pipe")};if n,e:=os.Stdout.Write(header);e!=nil||n!=len(header){return fail("native_output_pipe")};if n,e:=os.Stdout.Write(body);e!=nil||n!=len(body){return fail("native_output_pipe")};if e=os.Stdout.Close();e!=nil{return e}
 // The same frozen output FD remains open through the consumer's member checks.
 var readAck ackRead
 select{case readAck=<-ackChannel:case <-operationCtx.Done():select{case readAck=<-ackChannel:default:return fail("native_output_ack_missing")}}
 if readAck.err!=nil{return readAck.err};ack:=readAck.value
 if ack.Kind=="accept"&&ack.SHA256!=receipt.SHA256{return fail("native_output_ack_hash")}
 afterAck,e:=stat(f);if e!=nil||!same(after,afterAck)||after.Size!=afterAck.Size||after.Mtim!=afterAck.Mtim||after.Ctim!=afterAck.Ctim{return fail("native_output_changed_before_ack")}
 if e=f.Close();e!=nil{return e};cleanupErr:=cleanup();final:=struct{Nonce string;CleanupVerified bool;Accepted bool}{launch.Nonce,cleanupErr==nil,ack.Kind=="accept"};finalBytes,_:=json.Marshal(final);fmt.Fprintf(os.Stderr,"native_owner_final=%s\n",finalBytes)
 if cleanupErr!=nil{return cleanupErr};if ack.Kind!="accept"{return fail("native_consumer_rejected")};return nil

}
func main(){if len(os.Args)==2&&os.Args[1]=="--job-flight"{if e:=jobFlight();e!=nil{fmt.Fprintln(os.Stderr,"job_flight_failed="+e.Error());os.Exit(1)};return};if len(os.Args)==2&&os.Args[1]=="--internal-job-init"{os.Exit(jobInit())};if len(os.Args)==2&&os.Args[1]=="--internal-owner-init"{os.Exit(internalInit())};if e:=run();e!=nil{fmt.Fprintln(os.Stderr,"native_owner_failed="+e.Error());os.Exit(1)}}
