## IndyMAT SISO continuous-time representation.
## MATLAB measurement: olc26-control.txt rows ZP01.
function [keys, descriptions] = __sys_keys__ (sys, varargin)
  keys = {'Z';'P';'K';'DisplayFormat';'Variable';'IODelay';'InputDelay';'OutputDelay';'InputName';'InputUnit';'InputGroup';'OutputName';'OutputUnit';'OutputGroup';'Notes';'UserData';'Name';'Ts';'TimeUnit';'SamplingGrid'}; descriptions = repmat ({'SISO continuous-time property'},size(keys));
endfunction
