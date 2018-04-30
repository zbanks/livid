#![allow(dead_code)]
#[macro_use]
extern crate dlopen_derive;
extern crate dlopen;
use dlopen::wrapper::{Container,WrapperApi};
//use dlopen::symbor::{Library, Symbol, RefMut, SymBorApi};

use std::fs;
use std::fmt;
use std::slice;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::Seek;
use std::io::SeekFrom;
use std::ffi::{CString, CStr};
use std::path;
use std::marker::PhantomData;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CellType {
    Text = 0,
    Long = 1,
    Time = 2,
    Double = 3,
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
                    CellType::Time=> write!(f, "Time {:?}", self.value.time),
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
                .map(|(i, h)| Column { name: CString::new(h).unwrap(), index: i, cell_type: CellType::Text })
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
        if rc.is_ok() && rc.unwrap() > 0{
            self.cells = self.line
                .trim()
                .split(self.delimiter)
                .map(|s| CString::new(s).unwrap())
                .collect();
            return Option::Some(self.cells
                .iter()
                .zip(&self.output_columns)
                .map(|(s, c)| Cell { column: &c, empty: false, value: CellValue { text: CStrPtr { ptr: s.as_ptr(), phantom: PhantomData } } })
                .collect()
            )
        } else {
            return Option::None;
        }
    }
    
    fn reset(&'a mut self) {
        self.reader.seek(SeekFrom::Start(self.first_line_offset)).unwrap();
    }

    fn grid(&'a mut self, cells: &'a[Cell<'a, 'a>]) {
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
        (*api).input.next().map_or(std::ptr::null_mut(), |mut v| v.as_mut_ptr())
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
    columns: *const *const CColumn,
    columns_count: &'a usize,
    //columns_count: &'a mut usize,
    //setup: extern "C" fn(col_cnt: usize, cols: *mut CColumn) -> (),
    run: extern "C" fn(api: &'a LividApi<'a>) -> (),
}

fn run_livid(mut input: InputFile) -> () {
    let api = LividApi::new(&mut input);
    let container: Container<LividLib> = unsafe {
            Container::load("c_src/liblivid.so")
        }.unwrap();

    let output_columns = unsafe {
        slice::from_raw_parts(*container.columns, *container.columns_count)
    }.to_vec();
    println!("Columns: {:?}", output_columns);
    api.input.load_output_columns(output_columns);
    api.input.reset();
    container.run(&api);
}
    
fn main() {
    let workspace = path::Path::new("./wkspace");
    fs::create_dir_all(&workspace).unwrap();

    //let script_file_path = workspace.join("script.c");
    //let mut script_file = File::create(&script_file_path).unwrap();

    //let log_file_path = workspace.join("log");
    //let mut log_file = File::create(&log_file_path).unwrap();

    //let stdin = io::stdin();
    //let mut stdin_lock = stdin.lock();

    let mut input = InputFile::new(path::Path::new("test.csv")).unwrap();

    println!("Header: {:#?}", input.input_columns());
    //println!("Row: {:#?}", input.next());

    run_livid(input);
}
