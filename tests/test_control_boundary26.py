"""Original-operand scope checks for the new control representations.

These are IndyMAT rejection-boundary tests, not MATLAB rejection claims.
The native CT/SISO result matrix is measured in olc26-control.txt operator
rows; discrete/MIMO forms are outside the new representation subset.
"""
from tests import test_control_pid26 as pid_tests


class ControlBoundary26Tests(pid_tests.ControlPID26Tests):
    def test_original_operand_scope_both_orders(self):
        self.octave('''
        ops={'plus','minus','mtimes','mrdivide','feedback','series','parallel'};
        for q={pid(2,3,4,.2),pid(2),zpk(-1,-2,3)}
          q=q{1};
          for item={ss(tf(1,[1 2],.1)),ss(eye(2)),tf(1,[1 2],.1),tf(eye(2))}
            bad=item{1}; expected='IndyMAT:control:UnsupportedDiscreteTime'; if(!isequal(size(bad),[1 1])),expected='IndyMAT:control:UnsupportedMIMO';endif
            for op=ops
              for reverse=0:1
                caught=false; try
                  if(reverse),r=feval(op{1},bad,q);else,r=feval(op{1},q,bad);endif
                catch e,caught=true;assert(e.identifier,expected);end_try_catch
                assert(caught);
              endfor
            endfor
          endfor
        endfor
        ''')

    def test_native_ct_siso_and_native_only_paths(self):
        self.octave('''
        ops={'plus','minus','mtimes','mrdivide','feedback','series','parallel'};w=[.1 1 10]; s=ss(tf(1,[1 2]));
        for q={pid(2,3,4,.2),pid(2),zpk(-1,-3,2)}
          q=q{1};
          for op=ops
            for reverse=0:1
              if(reverse),a=s;b=q;else,a=q;b=s;endif
              r=feval(op{1},a,b); expected=feval(op{1},tf(a),tf(b)); assert(class(r),'ss');assert(freqresp(r,w),freqresp(expected,w),1e-9);
            endfor
          endfor
        endfor
        ## Native tf/ss only MIMO and discrete cases retain their algorithms.
        d=ss(tf(1,[1 2],.1));for op=ops,r=feval(op{1},d,d);assert(r.tsam,.1);endfor
        m=ss(eye(2));for op=ops,r=feval(op{1},m,m);assert(size(r),[2 2]);endfor
        ''')
