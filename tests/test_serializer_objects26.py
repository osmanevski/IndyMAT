"""Public object payload contracts exercised only in real octave-cli."""
import shutil
import subprocess
import unittest
from pathlib import Path

from scripts import compat

ROOT = Path(__file__).resolve().parents[1]
OCTAVE = shutil.which('octave-cli')


@unittest.skipUnless(OCTAVE, 'octave-cli is unavailable')
class ObjectPayload26Tests(unittest.TestCase):
    def run_octave(self, body):
        setup = compat.setup().replace("set(0,'defaultfigurevisible','off'); ", "")
        setup += f"addpath({compat.quote(ROOT / 'uyumluluk' / 'yardimcilar')}); "
        result = subprocess.run([OCTAVE, '--quiet', '--no-init-file', '--eval',
                                 setup + body], cwd=ROOT, text=True,
                                capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_duration_values_missing_shape_and_format(self):
        self.run_octave("x=seconds([1 NaN; Inf -2]); [p,s]=uy_ozet_nesne(x,100); "
                        "assert(s && isstruct(p)); assert(isequaln(p.Seconds,[1 NaN;Inf -2])); "
                        "assert(strcmp(p.Format,x.Format)); "
                        "y=x; y.Format='hh:mm:ss'; [q,t]=uy_ozet_nesne(y,100); "
                        "assert(t && !isequaln(p,q)); assert(isequaln(p.Seconds,q.Seconds)); "
                        "[q,t]=uy_ozet_nesne(seconds([1 NaN;Inf -3]),100); "
                        "assert(t && !isequaln(p,q)); "
                        "[q,t]=uy_ozet_nesne(seconds([]),100); assert(t && isempty(q.Seconds));")

    def test_categorical_values_categories_and_missing(self):
        self.run_octave("x=categorical([1 NaN 2],[1 2 3],{'a','b','unused'}); "
                        "[p,s]=uy_ozet_nesne(x,100); assert(s); "
                        "assert(isequaln(p.Codes,[1 NaN 2])); "
                        "assert(isequal(p.Categories,{'a';'b';'unused'})); "
                        "assert(!p.Ordinal && !p.Protected); "
                        "[q,t]=uy_ozet_nesne(x,100); assert(t && isequaln(p,q)); "
                        "[q,t]=uy_ozet_nesne(categorical([2 NaN 1],[1 2 3],{'a','b','unused'}),100); "
                        "assert(t && !isequaln(p,q)); "
                        "[q,t]=uy_ozet_nesne(categorical([1 NaN 2],[1 2 3],{'a','b','extra'}),100); "
                        "assert(t && !isequaln(p,q));")

    def test_categorical_flags(self):
        self.run_octave("x=categorical([1 2],[1 2],{'a','b'},'Ordinal',true); "
                        "[p,s]=uy_ozet_nesne(x,100); assert(s && p.Ordinal && p.Protected); "
                        "x=categorical([1 2],[1 2],{'a','b'},'Protected',true); "
                        "[q,t]=uy_ozet_nesne(x,100); assert(t && !q.Ordinal && q.Protected); "
                        "assert(!isequaln(p,q));")

    def test_budget_and_unsupported_kinds(self):
        self.run_octave("for x={seconds(1:5),categorical(1:5)}, "
                        "[p,s]=uy_ozet_nesne(x{1},4); assert(!s && isempty(p)); end; "
                        "for b=[0 -1 1.5 NaN Inf], "
                        "[p,s]=uy_ozet_nesne(seconds(1),b); assert(!s && isempty(p)); end; "
                        "[p,s]=uy_ozet_nesne(categorical(1,1,{'longname'}),4); assert(!s); "
                        "for x={1,struct('a',1),{1},@sin}, "
                        "[p,s]=uy_ozet_nesne(x{1},100); assert(!s && isempty(p)); end;")

    def test_table_values_and_complete_default_metadata(self):
        self.run_octave("x=table(int16([1;2]),[NaN 3;4 5],'VariableNames',{'a','b'}); "
                        "[p,s]=uy_ozet_nesne(x,1000); assert(s); "
                        "assert(isa(p.Values{1},'int16')); assert(isequal(size(p.Values{2}),[2 2])); "
                        "assert(isequaln(p.Values{2},[NaN 3;4 5])); "
                        "assert(isequal(p.Properties.VariableNames,{'a','b'})); "
                        "assert(isequal(p.Properties.DimensionNames,{'Row','Variables'})); "
                        "assert(isempty(fieldnames(p.Properties.CustomProperties))); "
                        "[q,t]=uy_ozet_nesne(x,1000); assert(t && isequaln(p,q)); "
                        "x.Properties.Description='changed'; x.Properties.UserData=struct('n',7); "
                        "[q,t]=uy_ozet_nesne(x,1000); assert(t && !isequaln(p,q)); "
                        "assert(strcmp(q.Properties.Description,'changed')); "
                        "assert(q.Properties.UserData.n==7); "
                        "x.Properties.RowNames={'r1','r2'}; x.Properties.VariableUnits={'m','s'}; "
                        "x.Properties.VariableDescriptions={'first','second'}; "
                        "[q,t]=uy_ozet_nesne(x,1000); assert(t); "
                        "assert(isequal(q.Properties.RowNames,x.Properties.RowNames)); "
                        "assert(isequal(q.Properties.VariableUnits,{'m','s'})); "
                        "assert(isequal(q.Properties.VariableDescriptions,{'first','second'}));")

    def test_table_rejects_custom_objects_and_budget_excess(self):
        self.run_octave("x=table(1); y=addprop(x,'extra','table'); "
                        "[p,s]=uy_ozet_nesne(y,1000); assert(!s && isempty(p)); "
                        "x.Properties.UserData=@sin; [p,s]=uy_ozet_nesne(x,1000); assert(!s); "
                        "x=table(seconds(1)); [p,s]=uy_ozet_nesne(x,1000); assert(!s); "
                        "x=table(zeros(2,100)); [p,s]=uy_ozet_nesne(x,100); assert(!s); "
                        "x=table(1); x.Properties.VariableContinuity={'continuous'}; "
                        "[p,s]=uy_ozet_nesne(x,1000); assert(!s);")

    def test_unzoned_datetime_exact_components_shape_and_format(self):
        self.run_octave("x=datetime([2020 2021;2022 2023],2,3,4,5,6.125); "
                        "[p,s]=uy_ozet_nesne(x,100); assert(s); "
                        "assert(isequal(size(p.Year),[2 2])); assert(isequal(p.Year,[2020 2021;2022 2023])); "
                        "assert(all(p.Second(:)==6.125)); assert(isempty(p.TimeZone)); "
                        "assert(strcmp(p.Format,x.Format)); "
                        "[q,t]=uy_ozet_nesne(x,100); assert(t && isequaln(p,q)); "
                        "y=x; y.Format='yyyy-MM-dd'; [q,t]=uy_ozet_nesne(y,100); "
                        "assert(t && !isequaln(p,q)); "
                        "for x={dateshift(datetime(2020,2,29),'start','month'), "
                        "dateshift(datetime(2020,2,29,12,0,0),'start','day'), "
                        "dateshift(datetime(2020,7,4),'start','year')}, "
                        "[p,s]=uy_ozet_nesne(x{1},100); assert(s); end;")

    def test_datetime_rejects_zones_missing_nonfinite_and_large_arrays(self):
        self.run_octave("for x={datetime(2020,1,1,'TimeZone','UTC'),NaT, "
                        "datetime(Inf,1,1),datetime(2020,1,1)+days(1:101)}, "
                        "[p,s]=uy_ozet_nesne(x{1},100); assert(!s && isempty(p)); end;")
