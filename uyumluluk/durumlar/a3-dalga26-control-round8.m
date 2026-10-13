% R2025b olcum-inc3-matlab.txt: named population, conversion, and empty name-only mutation.
%% wave26-zpk-empty-channel-names | zpk set tf ss
for name={'InputName','OutputName'}
  key=name{1}; label='u'; if strcmp(key,'OutputName'), label='y'; end
  q=set(zpk(),'Z',{-1},'P',{-2},'K',3,key,{label}); t=tf(q);
  assert(isequal(q.(key),{label})); assert(isequal(t.InputName,q.InputName)&&isequal(t.OutputName,q.OutputName));
  if strcmp(key,'InputName'), assert(isequal(q.OutputName,{''})); else, assert(isequal(q.InputName,{''})); end
end
q=set(zpk(),'Z',{-1},'P',{-2},'K',3,'InputName',{'u'},'OutputName',{'y'}); t=ss(q);
assert(isequal(t.InputName,{'u'})&&isequal(t.OutputName,{'y'}));
q=set(zpk(),'InputName',{'u'},'Z',{-1},'P',{-2},'K',3); t=tf(q); assert(isequal(t.InputName,{'u'}));
q=zpk(); q.InputName={'u'}; assert(isequal(size(q),[0 0]));
