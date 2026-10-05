function varargout=uy_yap_arg_output_repeat()
arguments (Output,Repeating)
varargout (1,1) double
end
varargout=repmat({single(7)},1,nargout);
end
