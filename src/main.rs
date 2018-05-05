#![allow(dead_code)]
extern crate dlopen;
#[macro_use]
extern crate dlopen_derive;
use dlopen::wrapper::{Container, WrapperApi};
//use dlopen::symbor::{Library, Symbol, RefMut, SymBorApi};

use std::fs;
use std::fmt;
use std::slice;
use std::fs::File;
use std::io;
use std::io::{BufRead, Seek, SeekFrom, Write};
use std::ffi::{CStr, CString};
use std::path;
use std::process::{Command, Stdio};
use std::os::unix::process::CommandExt;
use std::marker::PhantomData;

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
        match self {
            Text => "TEXT",
            Long => "LONG",
            Time => "TIME",
            Double => "DOUBLE",
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
struct CStrPtr<'v> {
    ptr: *const i8,
    phantom: PhantomData<&'v i8>,
}

#[repr(C)]
union CellValue<'v> {
    text: CStrPtr<'v>,
    long: i64,
    time: i64,
    double: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CColumn {
    name: *const char,
    cell_type: CellType,
    grid_show: bool,
    grid_width: usize,
}

#[derive(Debug)]
struct Column {
    name: CString,
    index: usize,
    cell_type: CellType,
}

#[repr(C)]
struct Cell<'col, 'val> {
    column: &'col CColumn,
    empty: bool,
    value: CellValue<'val>,
}

impl<'val> fmt::Debug for CStrPtr<'val> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        unsafe {
            // CStr uses `*const i8`, CString uses `*const u8`
            //let i8_ptr = std::mem::transmute::<*const u8, *const i8>(self.ptr);
            write!(f, "{:?}", CStr::from_ptr(self.ptr))
        }
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

//type Row<'col, 'val: 'col> = &'val [Cell<'col, 'val>];

/*
struct Row<'columns, 'values> {
    columns: &'columns Vec<Column>,
    empty: Vec<bool>,
    cells: Vec<CellValue<'cell>>,
}

impl<'col, 'val> fmt::Debug for Row<'col, 'val> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let cell_iter = self.empty
            .iter()
            .zip(self.empty)
            .zip(self.cells)
            .enumerate()
            .map(|(i, ((col, e), val))|
        fmt.debug_list().entries(cell_iter).finish();
        write!(f, "[{}] {} = ", self.column.index, self.column.name)?;
        if self.empty {
            write!(f, "Empty")
        } else {
            unsafe {
                match self.column.cell_type {
                    CellType::Text => write!(f, "Text {:?}", self.value.text),
                    CellType::Long => write!(f, "Long {:?}", self.value.long),
                    CellType::Time=> write!(f, "Time {:?}", self.value.time),
                    CellType::Double => write!(f, "Double {:?}", self.value.double),
                }
            }
        }
    }
}
*/

trait InputTable<'a> {
    fn input_columns(&'a self) -> &'a Vec<Column>;
    fn load_output_columns(&'a mut self, output_columns: Vec<CColumn>);
    fn next(&'a mut self) -> Option<Vec<Cell<'a, 'a>>>;
    fn reset(&'a mut self);
    fn grid(&'a mut self, cells: &'a [Cell<'a, 'a>]);
}

#[derive(Debug)]
struct InputFile {
    delimiter: char,
    header: String,
    line: String,
    first_line_offset: u64,
    reader: io::BufReader<File>,
    input_columns: Vec<Column>,
    output_columns: Vec<CColumn>,
    cells: Vec<CString>,
}

impl InputFile {
    fn new(input_path: &path::Path) -> io::Result<Self> {
        let input_file = File::open(&input_path)?;
        let mut input_reader = io::BufReader::new(input_file);
        let mut header = String::new();
        input_reader.read_line(&mut header)?;
        let offset = input_reader.seek(SeekFrom::Current(0)).unwrap();

        let delimiter = ',';
        let columns = header
            .trim()
            .split(delimiter)
            .enumerate()
            .map(|(i, h)| Column {
                name: CString::new(h).unwrap(),
                index: i,
                cell_type: CellType::Text,
            })
            .collect();

        return Result::Ok(InputFile {
            delimiter: delimiter,
            header: header.trim().to_string(),
            line: String::new(),
            first_line_offset: offset,
            reader: input_reader,
            input_columns: columns,
            output_columns: vec![],
            cells: vec![],
        });
    }
}

impl<'a> InputTable<'a> for InputFile {
    fn input_columns(&'a self) -> &'a Vec<Column> {
        return &self.input_columns;
    }

    fn load_output_columns(&'a mut self, output_columns: Vec<CColumn>) {
        self.output_columns = output_columns;
    }

    fn next(&'a mut self) -> Option<Vec<Cell<'a, 'a>>> {
        self.line.clear();
        let rc = self.reader.read_line(&mut self.line);
        if rc.is_ok() && rc.unwrap() > 0 {
            self.cells = self.line
                .trim()
                .split(self.delimiter)
                .map(|s| CString::new(s).unwrap())
                .collect();
            return Option::Some(
                self.cells
                    .iter()
                    .zip(&self.output_columns)
                    .map(|(s, c)| Cell {
                        column: &c,
                        empty: false,
                        value: CellValue {
                            text: CStrPtr {
                                ptr: s.as_ptr(),
                                phantom: PhantomData,
                            },
                        },
                    })
                    .collect(),
            );
        } else {
            return Option::None;
        }
    }

    fn reset(&'a mut self) {
        self.reader
            .seek(SeekFrom::Start(self.first_line_offset))
            .unwrap();
    }

    fn grid(&'a mut self, cells: &'a [Cell<'a, 'a>]) {
        println!(" GRID: {:?}", cells);
    }
}

#[repr(C)]
struct LividApi<'a> {
    next: extern "C" fn(api: *mut LividApi<'a>) -> *mut Cell<'a, 'a>,
    grid: extern "C" fn(api: *mut LividApi<'a>, cells: *const Cell<'a, 'a>) -> (),
    write: extern "C" fn(api: *mut LividApi<'a>, string: *const i8) -> (),
    input: &'a mut InputFile,
}

extern "C" fn livid_api_raw_next<'a>(api: *mut LividApi<'a>) -> *mut Cell<'a, 'a> {
    unsafe {
        (*api)
            .input
            .next()
            .map_or(std::ptr::null_mut(), |mut v| v.as_mut_ptr())
    }
}

extern "C" fn livid_api_raw_grid<'a>(api: *mut LividApi<'a>, cells: *const Cell<'a, 'a>) -> () {
    unsafe {
        let cell_slice = slice::from_raw_parts(cells, (*api).input.output_columns.len());
        (*api).input.grid(cell_slice)
    }
}

extern "C" fn livid_api_raw_write<'a>(_api: *mut LividApi<'a>, string: *const i8) -> () {
    print!("{}", unsafe { CStr::from_ptr(string) }.to_str().unwrap());
}

impl<'a> LividApi<'a> {
    fn new(input: &'a mut InputFile) -> Self {
        LividApi {
            next: livid_api_raw_next,
            grid: livid_api_raw_grid,
            write: livid_api_raw_write,
            input: input,
        }
    }
}

#[derive(WrapperApi, Debug)]
struct LividLib<'a> {
    //columns: &'a [CColumn],
    columns: *const CColumn,
    columns_count: &'a usize,
    //columns_count: &'a mut usize,
    //setup: extern "C" fn(col_cnt: usize, cols: *mut CColumn) -> (),
    run: extern "C" fn(api: &'a LividApi<'a>) -> (),
}

struct Editor {
    workspace: path::PathBuf,
    vimrc_path: path::PathBuf,
    script_file: File,
    log_file: File,
    output_file: File,
}

impl Editor {
    fn new() -> std::io::Result<Self> {
        let workspace = path::PathBuf::from("./wkspace");
        fs::create_dir_all(&workspace)?;

        let script_file_path = workspace.join("script.c");
        let mut script_file = File::create(&script_file_path)?;

        let log_file_path = workspace.join("log");
        let mut log_file = File::create(&log_file_path)?;

        let output_file_path = workspace.join("output");
        let mut output_file = File::create(&output_file_path)?;

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

        Ok(Editor {
            workspace: workspace,
            vimrc_path: vimrc_path,
            script_file: script_file,
            log_file: log_file,
            output_file: output_file,
        })
    }

    fn launch(&mut self) -> std::io::Result<()> {
        let vim_stdin = File::open("/dev/tty")?;
        let vim_stdout = File::create("/dev/tty")?;
        let vim_stderr = self.log_file.try_clone()?;
        Command::new("vim")
            .arg("--servername")
            .arg("livid")
            .arg("-S")
            .arg(self.vimrc_path.as_os_str())
            .stdin(Stdio::from(vim_stdin))
            .stdout(Stdio::from(vim_stdout))
            .stderr(Stdio::from(vim_stderr))
            .status();
        Ok(())
    }

    fn reload(&mut self) -> std::io::Result<()> {
        self.output_file.sync_all()?;
        Command::new("vim")
            .arg("--servername")
            .arg("livid")
            .arg("--remote-send")
            .arg("<Esc>:checktime<CR>")
            .status();
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
            .arg("-Ic_src")
            .arg("-fPIC")
            .arg("-shared")
            .arg("-o")
            .arg(&lib_path)
            .arg(self.workspace.join("script.c"))
            .status();
        Ok(lib_path)
    }
}

fn run_livid(mut editor: Editor, mut input: InputFile) -> () {
    generate_script(&mut editor.script_file, input.input_columns()).unwrap();
    //editor.launch().unwrap();

    editor.output_file.set_len(0).unwrap();
    let lib_path = editor.compile().unwrap();

    println!("Compiled: {:?}", lib_path);
    let api = LividApi::new(&mut input);
    let container: Container<LividLib> = unsafe { Container::load(lib_path) }.unwrap();
    println!(
        "Loaded container: {:?} {:?}",
        container.columns, container.columns_count
    );

    let output_columns =
        unsafe { slice::from_raw_parts(container.columns, *container.columns_count) }.to_vec();
    println!("Columns: {:?}", output_columns);
    api.input.load_output_columns(output_columns);
    api.input.reset();
    container.run(&api);

    editor.reload().unwrap();
}

fn generate_script(file: &mut File, columns: &Vec<Column>) -> std::io::Result<()> {
    file.set_len(0)?;
    write!(file, "#define COLUMN_LIST \\\n");
    for column in columns {
        write!(
            file,
            "    COLUMN({:16}, {:6}, SHOW) \\\n",
            column.name.to_str().unwrap(),
            column.cell_type.upper_str()
        )?;
    }
    write!(file, "\n")?;

    write!(file, "#include \"livid.h\"\n")?;
    file.write_all(
        br#"
void run(struct api * api) {
    /*
    write("%%zu %%zu %%zu", columns_cnt, sizeof(columns), sizeof(columns[0]));
    struct row row;
    while (next()) {
        //load_all(&row);
        //write("a=%%s b=%%s c=%%s", row.a, row.b, row.c);
        //grid(&row);
    }
    */
    printf("hello\n");
    struct cell * cells = NULL;
    while ((cells = api->next(api)) != NULL) {
        for (size_t i = 0; i < columns_count; i++) {
            //printf(" > %zu %zu %s %d\n",
            //       i, columns[i].index, columns[i].name, columns[i].cell_type);
            printf("[%zu %s %d] = '%s', ",
                    i, columns[i].name, columns[i].cell_type, cells[i].value.cell_text);
        }
        printf("\n");
        api->grid(api, cells);
    }
}
"#,
    )?;

    file.sync_all()?;
    Ok(())
}

fn main() {
    //let stdin = io::stdin();
    //let mut stdin_lock = stdin.lock();

    let mut editor = Editor::new().unwrap();
    let mut input = InputFile::new(path::Path::new("test.csv")).unwrap();

    println!("Header: {:#?}", input.input_columns());
    //println!("Row: {:#?}", input.next());

    run_livid(editor, input);
}
