#![allow(dead_code)]
extern crate dlopen;
#[macro_use]
extern crate dlopen_derive;
use dlopen::wrapper::{Container, WrapperApi};
extern crate inotify;
extern crate libc;
extern crate structopt;
use structopt::StructOpt;

use std::ffi::{CStr, CString};
use std::fmt;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{BufRead, Seek, SeekFrom, Write};
use std::path;
use std::process::{Command, Stdio};
use std::slice;
use std::str::FromStr;
use std::thread;
use std::default::Default;
use std::marker::PhantomData;
use std::os::raw::c_char;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::time::{Duration, Instant};
// TODO: non-zero default values for numerics
// TODO: parse time, time fns
// TODO: serialize stdin back out to workspace?

type Result<T> = std::result::Result<T, Box<std::error::Error>>;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CellType {
    Text = 0,
    Long = 1,
    Time = 2,
    Double = 3,
}

impl CellType {
    fn upper_str(self: &Self) -> &'static str {
        match *self {
            CellType::Text => "TEXT",
            CellType::Long => "LONG",
            CellType::Time => "TIME",
            CellType::Double => "DOUBLE",
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct CStrPtr<'v> {
    ptr: *const i8,
    phantom: PhantomData<&'v i8>,
}

impl<'a> CStrPtr<'a> {
    fn from(s: &'a CString) -> CStrPtr<'a> {
        CStrPtr {
            ptr: s.as_ptr(),
            phantom: PhantomData,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
union CellValue<'v> {
    text: CStrPtr<'v>,
    long: i64,
    time: i64,
    double: f64,
}

#[derive(Debug)]
struct Column {
    name: CString,
    index: usize,
    cell_type: CellType,
    grid_width: i16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CColumn {
    name: *const c_char,
    cell_type: CellType,
    grid_width: i16,
}

impl<'v> CellValue<'v> {
    fn to_string(&self, t: CellType, empty: bool) -> String {
        if empty {
            String::from("")
        } else {
            unsafe {
                match t {
                    CellType::Text => self.text.to_string(),
                    CellType::Long => self.long.to_string(),
                    CellType::Time => self.time.to_string(), // TODO
                    CellType::Double => self.double.to_string(),
                }
            }
        }
    }
}

impl Column {
    fn from_c(c: CColumn, index: usize) -> Self {
        Column {
            name: CString::from(unsafe { const_char_cstr(c.name) }),
            index: index,
            cell_type: c.cell_type,
            grid_width: c.grid_width,
        }
    }
    fn empty_value<'c>(&'c self) -> Cell<'c, 'c> {
        Cell {
            column: self,
            empty: true,
            value: CellValue { long: 0 },
        }
    }
    fn parse_value<'v, 'c: 'v>(&'c self, v: &'v CString) -> Cell<'c, 'v> {
        let op_value = match self.cell_type {
            CellType::Text => Some(CellValue {
                text: CStrPtr::from(v),
            }),
            CellType::Long => v
                .to_str()
                .ok()
                .and_then(|x| i64::from_str(x).ok())
                .map(|x| CellValue { long: x }),
            CellType::Time => v
                .to_str()
                .ok()
                .and_then(|x| i64::from_str(x).ok())
                .map(|x| CellValue { time: x }),
            CellType::Double => v
                .to_str()
                .ok()
                .and_then(|x| f64::from_str(x).ok())
                .map(|x| CellValue { double: x }),
        };
        if let Some(value) = op_value {
            Cell {
                column: self,
                empty: false,
                value: value,
            }
        } else {
            self.empty_value()
        }
    }
}

struct Cell<'col, 'val> {
    column: &'col Column,
    empty: bool,
    value: CellValue<'val>,
}

type Row<'col, 'val> = Vec<Cell<'col, 'val>>;

impl<'val> fmt::Debug for CStrPtr<'val> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", unsafe { const_char_cstr(self.ptr) })
    }
}

unsafe fn const_char_cstr<'a>(ptr: *const c_char) -> &'a CStr {
    if ptr == std::ptr::null() {
        Default::default()
    } else {
        CStr::from_ptr(ptr)
    }
}

impl<'val> CStrPtr<'val> {
    fn to_string(&'val self) -> String {
        unsafe { const_char_cstr(self.ptr) }
            .to_str()
            .unwrap()
            .to_string()
    }
}

impl<'col, 'val> fmt::Debug for Cell<'col, 'val> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.empty {
            write!(f, "Empty")
        } else {
            unsafe {
                match self.column.cell_type {
                    CellType::Text => write!(f, "Text {:?}", self.value.text),
                    CellType::Long => write!(f, "Long {:?}", self.value.long),
                    CellType::Time => write!(f, "Time {:?}", self.value.time),
                    CellType::Double => write!(f, "Double {:?}", self.value.double),
                }
            }
        }
    }
}

trait InputTable<'a> {
    fn input_columns(&'a self) -> &'a Vec<Column>;
    fn output_columns(&'a self) -> &'a Vec<Column>;
    fn set_output_columns(&'a mut self, output_columns: Vec<Column>);
    fn next(&'a mut self) -> Option<Vec<Cell<'a, 'a>>>;
    fn reset(&'a mut self);
}

#[derive(Debug)]
struct CsvInputFile {
    delimiter: char,
    header: String,
    line: String,
    reader: io::BufReader<File>,
    input_columns: Vec<Column>,
    output_columns: Vec<Column>,
    output_input_map: Vec<Option<usize>>,
    row_index: usize,
    raw_cells: Vec<Vec<CString>>,
}

impl CsvInputFile {
    fn new(input_path: &path::Path, delimiter: char) -> Result<Self> {
        let input_file = File::open(&input_path)?;
        let mut input_reader = io::BufReader::new(input_file);
        let mut header = String::new();
        input_reader.read_line(&mut header)?;

        let columns = header
            .trim()
            .split(delimiter)
            .enumerate()
            .map(|(i, h)| Column {
                name: CString::new(h).unwrap(),
                index: i,
                cell_type: CellType::Text,
                grid_width: 0,
            }).collect();

        return Ok(CsvInputFile {
            delimiter: delimiter,
            header: header.trim().to_string(),
            line: String::new(),
            reader: input_reader,
            input_columns: columns,
            output_input_map: vec![],
            output_columns: vec![],
            row_index: 0,
            raw_cells: vec![],
        });
    }
}

impl<'a> InputTable<'a> for CsvInputFile {
    fn input_columns(&'a self) -> &'a Vec<Column> {
        &self.input_columns
    }

    fn output_columns(&'a self) -> &'a Vec<Column> {
        &self.output_columns
    }

    fn set_output_columns(&'a mut self, output_columns: Vec<Column>) {
        self.output_columns = output_columns;
        self.output_input_map = self
            .output_columns
            .iter()
            .map(|oc| {
                self.input_columns
                    .iter()
                    .find(|ic| ic.name == oc.name)
                    .map(|ic| ic.index)
            }).collect();
    }

    fn next(&'a mut self) -> Option<Vec<Cell<'a, 'a>>> {
        let raw_cells = &mut self.raw_cells;
        let line_buf = &mut self.line;
        let delimiter = self.delimiter;
        let reader = &mut self.reader;
        let input_len = self.input_columns.len();
        let output_input_map: &Vec<_> = &self.output_input_map;
        let output_columns = &self.output_columns;
        let raw_row = if raw_cells.len() > self.row_index {
            raw_cells.get(self.row_index)
        } else {
            line_buf.clear();
            reader.read_line(line_buf).ok().and_then(move |rc| {
                if rc <= 0 {
                    return None;
                }
                let mut l: Vec<CString> = line_buf
                    .trim()
                    .split(delimiter)
                    .map(|s| CString::new(s).unwrap())
                    .collect();
                l.resize(input_len, CString::default());
                raw_cells.push(l);
                raw_cells.last()
            })
        }?;
        self.row_index += 1;
        Some(
            output_input_map
                .iter()
                .zip(output_columns.iter())
                .map(|(opt_idx, col)| {
                    opt_idx
                        .and_then(|x| raw_row.get(x))
                        .map(|x| col.parse_value(x))
                        .unwrap_or(col.empty_value())
                }).collect(),
        )
    }

    fn reset(&'a mut self) {
        self.row_index = 0;
    }
}

#[repr(C)]
struct LividApi<'a> {
    next: extern "C" fn(api: *mut LividApi<'a>, row_out: *mut CellValue<'a>, empty_out: *mut i8) -> i8,
    grid: extern "C" fn(api: *mut LividApi<'a>, row: *const CellValue<'a>, empty: *const i8) -> i8,
    write: extern "C" fn(api: *mut LividApi<'a>, string: *const c_char) -> (),
    input: &'a mut CsvInputFile,
    editor: &'a mut Editor,
}

extern "C" fn livid_api_raw_next<'a>(api: *mut LividApi<'a>, row_out: *mut CellValue<'a>, empty_out: *mut i8) -> i8 {
    unsafe {
        if let Some(row) = (*api).input.next() {
            for (i, cell) in row.iter().enumerate() {
                row_out.add(i).write(cell.value.clone());
                empty_out.add(i).write(cell.empty as i8);
            }
            1
        } else {
            0
        }
    }
}

extern "C" fn livid_api_raw_grid<'a>(api: *mut LividApi<'a>, row: *const CellValue<'a>, empty: *const i8) -> i8 {
    unsafe {
        let api = &mut (*api);
        let columns = &api.input.output_columns();
        let row_slice = slice::from_raw_parts(row, columns.len());
        let empty_slice = slice::from_raw_parts(empty, columns.len());
        api.editor
            .grid(columns, row_slice, empty_slice)
            .map(|x| x as i8)
            .unwrap_or(-1)
    }
}

extern "C" fn livid_api_raw_write<'a>(api: *mut LividApi<'a>, string: *const i8) -> () {
    unsafe {
        (*api)
            .editor
            .write(const_char_cstr(string).to_str().unwrap())
            .unwrap()
    }
}

impl<'a> LividApi<'a> {
    fn new(input: &'a mut CsvInputFile, editor: &'a mut Editor) -> Self {
        LividApi {
            next: livid_api_raw_next,
            grid: livid_api_raw_grid,
            write: livid_api_raw_write,
            input: input,
            editor: editor,
        }
    }
}

#[derive(WrapperApi, Debug)]
struct LividLib<'a> {
    columns: *const CColumn,
    columns_count: &'a usize,
    grid_rows_limit: &'a usize,
    run: extern "C" fn(api: &'a LividApi<'a>) -> (),
}

struct StdioRedirector {
    stdout_fd: RawFd,
    stderr_fd: RawFd,
}
impl StdioRedirector {
    fn new(target_fd: RawFd) -> Self{
        unsafe {
            let stdout_fd = libc::dup(1);
            let stderr_fd = libc::dup(2);
            libc::close(1);
            libc::dup(target_fd);
            libc::close(2);
            libc::dup(target_fd);
            StdioRedirector {
                stdout_fd: stdout_fd,
                stderr_fd: stderr_fd,
            }
        }
    }
}
impl Drop for StdioRedirector {
    fn drop(&mut self) {
        unsafe {
            libc::close(1);
            libc::dup(self.stdout_fd);
            libc::close(self.stdout_fd);
            libc::close(2);
            libc::dup(self.stderr_fd);
            libc::close(self.stderr_fd);
        }
    }
}

struct Editor {
    workspace: path::PathBuf,
    vimrc_path: path::PathBuf,
    script_file: File,
    log_file: File,
    output_file: File,
    script_notify: inotify::Inotify,
    grid_rows: usize,
    grid_rows_limit: usize,
    auto_widths: Vec<usize>,
    redirector: StdioRedirector,
    last_reload: Instant,
}

impl Editor {
    fn new() -> Result<Self> {
        let workspace = path::PathBuf::from("./wkspace");
        fs::create_dir_all(&workspace)?;

        let header_file_path = workspace.join("livid.h");
        let mut header_file = File::create(&header_file_path)?;
        header_file.write_all(include_str!("../c_src/livid.h").as_bytes())?;

        let script_file_path = workspace.join("script.c");
        let script_file = File::create(&script_file_path)?;
        let mut script_notify = inotify::Inotify::init()?;
        script_notify.add_watch(script_file_path.clone(), inotify::WatchMask::CLOSE_WRITE)?;

        let log_file_path = workspace.join("log");
        let log_file = File::create(&log_file_path)?;

        let output_file_path = workspace.join("output");
        let output_file = File::create(&output_file_path)?;

        let vimrc_path = workspace.join("vimrc");
        {
            let mut vimrc = File::create(&vimrc_path)?;
            write!(vimrc, "set backupcopy=yes\n")?;
            write!(vimrc, "set autoread\n")?;
            write!(vimrc, "set splitbelow\n")?;
            write!(vimrc, "edit {}\n", output_file_path.to_str().unwrap())?;
            write!(vimrc, "split {}\n", log_file_path.to_str().unwrap())?;
            write!(vimrc, "vsplit {}\n", script_file_path.to_str().unwrap())?;
        }

        let log_fd = log_file.as_raw_fd();

        Ok(Editor {
            workspace: workspace,
            vimrc_path: vimrc_path,
            script_file: script_file,
            script_notify: script_notify,
            log_file: log_file,
            output_file: output_file,
            grid_rows: 0,
            grid_rows_limit: 20,
            auto_widths: vec![],
            redirector: StdioRedirector::new(log_fd),
            last_reload: Instant::now(),
        })
    }

    fn launch(&mut self) -> Result<thread::JoinHandle<()>> {
        let vim_stdin = File::open("/dev/tty")?;
        let vim_stdout = File::create("/dev/tty")?;
        let vim_stderr = self.log_file.try_clone()?;
        let vimrc_path = self.vimrc_path.clone();
        Ok(thread::spawn(move || {
            Command::new("vim")
                .arg("--servername")
                .arg("livid")
                .arg("-S")
                .arg(vimrc_path.as_os_str())
                .stdin(Stdio::from(vim_stdin))
                .stdout(Stdio::from(vim_stdout))
                .stderr(Stdio::from(vim_stderr))
                .status()
                .unwrap();
        }))
    }

    fn reload(&mut self, force: bool) -> Result<()> {
        let now = Instant::now();
        if force || now > self.last_reload + Duration::from_millis(100) {
            self.last_reload = now;
            self.output_file.sync_all()?;
            Command::new("vim")
                .arg("--servername")
                .arg("livid")
                .arg("--remote-send")
                .arg("<Esc>:checktime<CR>")
                .status()?;
        }
        Ok(())
    }

    fn compile(&mut self) -> std::io::Result<path::PathBuf> {
        let lib_path = self.workspace.join("liblivid.so").to_path_buf();
        Command::new("/usr/bin/gcc")
            .arg("-std=c99")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-Wconversion")
            .arg("-Werror")
            .arg("-O0")
            .arg("-ggdb3")
            .arg("-D_POSIX_C_SOURCE=201704L")
            .arg("-fPIC")
            .arg("-shared")
            .arg("-o")
            .arg(&lib_path)
            .arg(self.workspace.join("script.c"))
            .stderr(self.log_file.try_clone()?)
            .status()?;
        Ok(lib_path)
    }

    fn grid<'a>(&mut self, columns: &Vec<Column>, values: &[CellValue<'a>], emptys: &[i8]) -> Result<bool> {
        assert!(columns.len() == values.len());
        assert!(columns.len() == emptys.len());

        if self.auto_widths.len() < columns.len() {
            self.auto_widths.resize(columns.len(), 0);
        }
        if self.grid_rows == 0 {
            for (column, auto_width) in columns.iter()
                .zip(self.auto_widths.iter_mut()) {
                let grid_width = column.grid_width;
                let width = if grid_width < 0 {
                    continue;
                } else if grid_width == 0 {
                    *auto_width
                } else {
                    grid_width as usize
                };
                let string_value = column.name.to_str().unwrap();
                write!(
                    self.output_file,
                    "| {val:>width$} ",
                    width = width,
                    val = string_value
                )?;
                *auto_width = std::cmp::max(*auto_width, string_value.len());
            }
            write!(self.output_file, "|\n")?;

            for (column, auto_width) in columns.iter()
                .zip(self.auto_widths.iter_mut()) {
                let grid_width = column.grid_width;
                let width = if grid_width < 0 {
                    continue;
                } else if grid_width == 0 {
                    *auto_width
                } else {
                    grid_width as usize
                };
                let dashes = "-".repeat(width + 2);
                write!(self.output_file, "+{}", dashes)?;
            }
            write!(self.output_file, "+\n")?;
        }
        if self.grid_rows >= self.grid_rows_limit {
            if self.grid_rows == self.grid_rows_limit {
                write!(self.output_file, "------\nHit limit of {} rows\n", self.grid_rows_limit)?;
                self.reload(true)?;
            }
            return Ok(true);
        }
        self.grid_rows += 1;
        for (((column, value), empty), auto_width) in columns.iter()
            .zip(values.iter())
            .zip(emptys.iter().map(|x| *x != 0))
            .zip(self.auto_widths.iter_mut()) {
            let grid_width = column.grid_width;
            let width = if grid_width < 0 {
                continue;
            } else if grid_width == 0 {
                *auto_width
            } else {
                grid_width as usize
            };
            let string_value = value.to_string(column.cell_type, empty);
            write!(
                self.output_file,
                "| {val:>width$} ",
                width = width,
                val = string_value
            )?;
            *auto_width = std::cmp::max(*auto_width, string_value.len());
        }
        write!(self.output_file, "|\n")?;
        self.reload(false)?;
        Ok(false)
    }

    fn write(&mut self, string: &str) -> Result<()> {
        write!(self.output_file, "{}", string)?;
        Ok(())
    }

    fn reset_output(&mut self) -> std::io::Result<()> {
        self.output_file.set_len(0)?;
        self.output_file.seek(SeekFrom::Start(0))?;
        self.log_file.set_len(0)?;
        self.log_file.seek(SeekFrom::Start(0))?;
        self.grid_rows = 0;
        Ok(())
    }

    fn set_grid_rows_limit(&mut self, limit: usize) -> () {
        self.grid_rows_limit = limit;
    }
}

fn run_livid(mut editor: Editor, mut input: CsvInputFile) -> Result<()> {
    generate_script(&mut editor.script_file, input.input_columns())?;
    let _editor_jh = editor.launch()?;
    loop {
        editor.reset_output()?;
        let lib_path = editor.compile()?;

        println!("Compiled: {:?}", lib_path);
        {
            let api = LividApi::new(&mut input, &mut editor);
            let container: Container<LividLib> = unsafe { Container::load(lib_path) }.unwrap();
            println!(
                "Loaded container: {:?} {:?}",
                container.columns, container.columns_count
            );
            api.editor.set_grid_rows_limit(*container.grid_rows_limit);

            let output_columns =
                unsafe { slice::from_raw_parts(container.columns, *container.columns_count) }
                    .iter()
                    .enumerate()
                    .map(|(i, c)| { Column::from_c(*c, i) }) 
                    .collect();
            println!("Columns: {:?}", output_columns);
            api.input.set_output_columns(output_columns);
            api.input.reset();
            container.run(&api);
        }

        editor.reload(true).unwrap();

        let mut buffer = [0; 1024];
        let _events = editor.script_notify.read_events_blocking(&mut buffer);
    }
}

fn generate_script(file: &mut File, columns: &Vec<Column>) -> Result<()> {
    file.set_len(0)?;
    write!(file, "#define COLUMN_LIST \\\n")?;
    write!(
        file,
        "    /*     {:16}  {:10}  {:10} */\\\n",
        "column name", "type", "grid width"
    )?;
    for column in columns {
        write!(
            file,
            "    COLUMN({:16}, {:10}, {:10}) \\\n",
            column.name.to_str().unwrap(),
            column.cell_type.upper_str(),
            "GRID_AUTO"
        )?;
    }
    write!(file, "\n")?;

    file.write_all(include_str!("../c_src/template.c").as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let editor = Editor::new()?;
    let input = CsvInputFile::new(&opt.input, opt.delimiter)?;
    println!("Header: {:#?}", input.input_columns());

    run_livid(editor, input)
}

#[derive(StructOpt, Debug)]
#[structopt(name = "livid")]
struct Opt {
    /// Input CSV file
    #[structopt(name = "file", default_value = "/dev/stdin", parse(from_os_str))]
    input: path::PathBuf,

    #[structopt(short = "d", long = "delimiter", default_value = ",")]
    delimiter: char,
}

