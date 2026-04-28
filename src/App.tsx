import React, { useState, useEffect, useRef } from 'react';
import { 
  Plus, 
  Trash2, 
  Save, 
  Printer, 
  Settings, 
  Image as ImageIcon,
  Check,
  X,
  RotateCw,
  RotateCcw,
  LayoutGrid,
  AlertCircle,
  Loader2,
  ChevronDown,
  Eraser,
  MousePointer2,
  Square,
  Rows3,
  ListEnd
} from 'lucide-react';
import { motion, AnimatePresence } from 'motion/react';
import { invoke } from '@tauri-apps/api/core';

// --- Types ---

interface GridConfig {
  id: string;
  name: string;
  rows: number;
  cols: number;
  cellWidth: number; // mm
  cellHeight: number; // mm
  gapX: number; // mm
  gapY: number; // mm
  paddingTop: number; // mm
  paddingLeft: number; // mm
  pageName?: string;
  pageWidth?: number; // mm
  pageHeight?: number; // mm
  dpi?: number;
}

interface PhotoInstance {
  imageId: string;
  objectFit?: 'cover' | 'contain';
  alignment?: string; // 'top-left', 'center', etc.
  rotation?: number; // 0, 90, 180, 270
  outline?: boolean; // new
}

interface ImageAsset {
  id: string;
  url: string;
  name: string;
}

interface AppState {
  grid: GridConfig;
  images: ImageAsset[];
  cells: Record<string, PhotoInstance>; // key: "row-col"
  selectedImageId: string | null;
  savedTemplates: GridConfig[];
  imageConfigs: Record<string, Omit<PhotoInstance, 'imageId'>>;
}

type ApplyRange = 'cell' | 'row' | 'after';
type GridAction = 'place' | 'clear';

// --- Constants ---

const DEFAULT_GRID: GridConfig = {
  id: 'a4',
  name: 'A4',
  rows: 5,
  cols: 5,
  cellWidth: 35,
  cellHeight: 45,
  gapX: 5,
  gapY: 5,
  paddingTop: 5,
  paddingLeft: 10,
  pageName: 'A4',
  pageWidth: 210,
  pageHeight: 297,
  dpi: 600
};

const BUILT_IN_TEMPLATES: GridConfig[] = [
  DEFAULT_GRID,
  {
    ...DEFAULT_GRID,
    id: '4x6',
    name: '4x6',
    rows: 3,
    cols: 2,
    pageName: '4x6',
    pageWidth: 101.6,
    pageHeight: 152.4,
  },
];

const createCustomTemplateId = () => `custom-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;

const ALIGNMENT_MAP: Record<string, string> = {
  'top-left': '0% 0%',
  'top-center': '50% 0%',
  'top-right': '100% 0%',
  'center-left': '0% 50%',
  'center': '50% 50%',
  'center-right': '100% 50%',
  'bottom-left': '0% 100%',
  'bottom-center': '50% 100%',
  'bottom-right': '100% 100%'
};

const parseCellKey = (key: string) => {
  const [row, col] = key.split('-').map(Number);
  return { row, col };
};

const isCellInGrid = (key: string, grid: GridConfig) => {
  const { row, col } = parseCellKey(key);
  return Number.isInteger(row) && Number.isInteger(col) && row >= 0 && col >= 0 && row < grid.rows && col < grid.cols;
};

const pruneCellsToGrid = (cells: Record<string, PhotoInstance>, grid: GridConfig) => {
  return Object.fromEntries(
    Object.entries(cells).filter(([key]) => isCellInGrid(key, grid))
  ) as Record<string, PhotoInstance>;
};

const normalizeGridForCompare = (grid: GridConfig) => ({
  rows: grid.rows,
  cols: grid.cols,
  cellWidth: grid.cellWidth,
  cellHeight: grid.cellHeight,
  gapX: grid.gapX,
  gapY: grid.gapY,
  paddingTop: grid.paddingTop,
  paddingLeft: grid.paddingLeft,
  pageName: grid.pageName || '',
  pageWidth: grid.pageWidth || 210,
  pageHeight: grid.pageHeight || 297,
  dpi: grid.dpi || 600,
});

const gridsMatch = (a: GridConfig, b: GridConfig) => (
  JSON.stringify(normalizeGridForCompare(a)) === JSON.stringify(normalizeGridForCompare(b))
);

const mergeTemplates = (savedTemplates: GridConfig[] = []) => {
  const seenNames = new Set(BUILT_IN_TEMPLATES.map(template => template.name.trim().toLowerCase()));
  const customTemplates = savedTemplates.filter(template => {
    if (template.id === 'passport') return false;
    if (BUILT_IN_TEMPLATES.some(builtIn => builtIn.id === template.id)) return false;

    const nameKey = template.name.trim().toLowerCase();
    if (!nameKey || seenNames.has(nameKey)) return false;

    seenNames.add(nameKey);
    return true;
  });

  return [...BUILT_IN_TEMPLATES, ...customTemplates];
};

const templateNameExists = (templates: GridConfig[], name: string) => (
  templates.some(template => template.name.trim().toLowerCase() === name.trim().toLowerCase())
);

// --- IndexedDB Image Storage ---

const DB_NAME = 'PrestoPrintPro';
const DB_VERSION = 1;
const STORE_NAME = 'images';

const initDB = (): Promise<IDBDatabase> => {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = (e: any) => {
      const db = e.target.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: 'id' });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
};

const saveImagesToDB = async (images: ImageAsset[]) => {
  try {
    const db = await initDB();
    const tx = db.transaction(STORE_NAME, 'readwrite');
    const store = tx.objectStore(STORE_NAME);
    store.clear();
    images.forEach(img => store.put(img));
    return new Promise((resolve, reject) => {
      tx.oncomplete = resolve;
      tx.onerror = () => reject(tx.error);
    });
  } catch (e) {
    console.error("Failed to save images to IndexedDB:", e);
  }
};

const loadImagesFromDB = async (): Promise<ImageAsset[]> => {
  try {
    const db = await initDB();
    const tx = db.transaction(STORE_NAME, 'readonly');
    const store = tx.objectStore(STORE_NAME);
    const req = store.getAll();
    return new Promise((resolve, reject) => {
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  } catch (e) {
    console.error("Failed to load images from IndexedDB:", e);
    return [];
  }
};

// --- Components ---

function DraftInput({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const [localVal, setLocalVal] = useState(value.toString());

  useEffect(() => {
    setLocalVal(value.toString());
  }, [value]);

  return (
    <input 
      type="number" 
      className="w-20 px-2 py-1 rounded border border-slate-300 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none text-right font-medium" 
      value={localVal}
      onChange={e => {
        setLocalVal(e.target.value);
        if (e.target.value !== '') {
           onChange(parseFloat(e.target.value));
        }
      }}
    />
  );
}

export default function App() {
  const [state, setState] = useState<AppState>(() => {
    const saved = localStorage.getItem('presto_print_pro_state_v3');
    if (saved) {
      try {
        const parsed = JSON.parse(saved) as AppState;
        return {
          ...parsed,
          selectedImageId: null,
          savedTemplates: mergeTemplates(parsed.savedTemplates || []),
          imageConfigs: parsed.imageConfigs || {}
        };
      } catch (e) {
        console.error("Failed to load state", e);
      }
    }
    return {
      grid: { ...DEFAULT_GRID },
      images: [],
      cells: {},
      selectedImageId: null,
      savedTemplates: BUILT_IN_TEMPLATES,
      imageConfigs: {}
    };
  });

  // Modal States
  const [isPaperStyleOpen, setIsPaperStyleOpen] = useState(false);
  const [draftGrid, setDraftGrid] = useState<GridConfig | null>(null);
  const [pendingTemplateGrid, setPendingTemplateGrid] = useState<GridConfig | null>(null);
  const [templateNameDraft, setTemplateNameDraft] = useState('');
  const [templateNameError, setTemplateNameError] = useState('');
  const [editingPhotoId, setEditingPhotoId] = useState<string | null>(null);
  const [applyRange, setApplyRange] = useState<ApplyRange>('cell');
  const [gridAction, setGridAction] = useState<GridAction>('place');
  
  const [showPrintHint, setShowPrintHint] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const previewAreaRef = useRef<HTMLElement>(null);
  const [previewScale, setPreviewScale] = useState(1);

  // Printer states
  const [printers, setPrinters] = useState<{name: string; is_default: boolean}[]>([]);
  const [selectedPrinter, setSelectedPrinter] = useState<string>('');
  const [isPrinting, setIsPrinting] = useState(false);
  const [printStatus, setPrintStatus] = useState<{type: 'success' | 'error'; message: string} | null>(null);

  // Fetch printers on mount
  useEffect(() => {
    invoke<{name: string; is_default: boolean}[]>('cmd_list_printers')
      .then(list => {
        setPrinters(list);
        const def = list.find(p => p.is_default);
        if (def) setSelectedPrinter(def.name);
      })
      .catch(err => console.error('Failed to list printers:', err));
  }, []);
  // Load images from IndexedDB on mount
  useEffect(() => {
    loadImagesFromDB().then(dbImages => {
      if (dbImages && dbImages.length > 0) {
        setState(prev => {
          const existingIds = new Set(prev.images.map(img => img.id));
          const newImages = dbImages.filter(img => !existingIds.has(img.id));
          if (newImages.length === 0) return prev;
          return { ...prev, images: [...prev.images, ...newImages] };
        });
      }
    });
  }, []);

  // Sync state to LocalStorage and images to IndexedDB
  useEffect(() => {
    try {
      const stateToSave = { ...state, images: [] };
      localStorage.setItem('presto_print_pro_state_v3', JSON.stringify(stateToSave));
    } catch (e) {
      console.error("Failed to save state to localStorage:", e);
    }
    saveImagesToDB(state.images).catch(console.error);
  }, [state]);

  // --- Handlers ---

  const handleImageUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files) return;

    Array.from(files as FileList).forEach((file: File) => {
      const reader = new FileReader();
      reader.onload = (event) => {
        const newImage: ImageAsset = {
          id: Math.random().toString(36).substr(2, 9),
          url: event.target?.result as string,
          name: file.name
        };
        setState(prev => ({
          ...prev,
          images: [...prev.images, newImage],
          imageConfigs: {
            ...prev.imageConfigs,
            [newImage.id]: { objectFit: 'cover', alignment: 'center', rotation: 0, outline: false }
          }
        }));
      };
      reader.readAsDataURL(file);
    });
  };

  const deleteImage = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setState(prev => {
      const newImages = prev.images.filter(img => img.id !== id);
      const newCells = { ...prev.cells };
      Object.keys(newCells).forEach(key => {
        if (newCells[key].imageId === id) delete newCells[key];
      });
      const newConfigs = { ...prev.imageConfigs };
      delete newConfigs[id];
      return { ...prev, images: newImages, cells: newCells, imageConfigs: newConfigs, selectedImageId: prev.selectedImageId === id ? null : prev.selectedImageId };
    });
  };





  const getApplyRangeKeys = (key: string, grid: GridConfig) => {
    const { row, col } = parseCellKey(key);
    if (!isCellInGrid(key, grid)) return [];

    if (applyRange === 'row') {
      return Array.from({ length: grid.cols }, (_, c) => `${row}-${c}`);
    }

    if (applyRange === 'after') {
      const startIndex = row * grid.cols + col;
      return Array.from({ length: grid.rows * grid.cols - startIndex }, (_, offset) => {
        const index = startIndex + offset;
        return `${Math.floor(index / grid.cols)}-${index % grid.cols}`;
      });
    }

    return [key];
  };

  const stampCell = (key: string) => {
    const targetKeys = getApplyRangeKeys(key, state.grid);
    if (targetKeys.length === 0) return;

    if (gridAction === 'clear') {
      setState(prev => {
        const nextCells = { ...prev.cells };
        targetKeys.forEach(targetKey => {
          delete nextCells[targetKey];
        });
        return { ...prev, cells: nextCells };
      });
      return;
    }

    if (state.selectedImageId) {
      setState(prev => {
        const config = prev.imageConfigs[prev.selectedImageId!] || { objectFit: 'cover', alignment: 'center', rotation: 0, outline: false };
        const nextCells = { ...prev.cells };
        targetKeys.forEach(targetKey => {
          nextCells[targetKey] = { imageId: prev.selectedImageId!, ...config };
        });
        return {
          ...prev,
          cells: nextCells
        };
      });
    } else {
      if (state.cells[key]) {
         setEditingPhotoId(state.cells[key].imageId);
      }
    }
  };

  const applyPaperStyle = () => {
    if (!draftGrid) return;

    const existingMatch = state.savedTemplates.find(template => gridsMatch(template, draftGrid));
    if (existingMatch) {
      const nextGrid = { ...existingMatch };
      setState(prev => ({
        ...prev,
        grid: nextGrid,
        cells: pruneCellsToGrid(prev.cells, nextGrid)
      }));
      setIsPaperStyleOpen(false);
      return;
    }

    const isBuiltIn = BUILT_IN_TEMPLATES.some(t => t.id === draftGrid.id);
    
    if (isBuiltIn) {
      setPendingTemplateGrid({ ...draftGrid });
      setTemplateNameDraft('');
      setTemplateNameError('');
    } else {
      const nextGrid = { ...draftGrid };
      const nextTemplates = state.savedTemplates.map(t => 
        t.id === nextGrid.id ? nextGrid : t
      );
      
      setState(prev => ({
        ...prev,
        grid: nextGrid,
        savedTemplates: nextTemplates,
        cells: pruneCellsToGrid(prev.cells, nextGrid)
      }));
      setIsPaperStyleOpen(false);
    }
  };

  const saveNamedPaperStyle = () => {
    if (!pendingTemplateGrid) return;

    const templateName = templateNameDraft.trim();
    if (!templateName) {
      setTemplateNameError('Enter a paper style name.');
      return;
    }

    if (templateNameExists(state.savedTemplates, templateName)) {
      setTemplateNameError('A paper style with this name already exists.');
      return;
    }

    const nextGrid = {
      ...pendingTemplateGrid,
      id: createCustomTemplateId(),
      name: templateName,
      pageName: pendingTemplateGrid.pageName || templateName,
    };
    const nextTemplates = mergeTemplates([...state.savedTemplates, nextGrid]);

    setState(prev => ({
      ...prev,
      grid: nextGrid,
      savedTemplates: nextTemplates,
      cells: pruneCellsToGrid(prev.cells, nextGrid)
    }));
    setPendingTemplateGrid(null);
    setTemplateNameDraft('');
    setTemplateNameError('');
    setIsPaperStyleOpen(false);
  };

  const handlePrint = async () => {
    // Build the print job: collect unique images and map cells to them
    const activeCells = Object.entries(state.cells).filter(([key]) => isCellInGrid(key, state.grid));
    
    const images: Record<string, string> = {};
    const cells = activeCells.map(([key, cell]) => {
      const { row, col } = parseCellKey(key);
      const imageAsset = state.images.find(img => img.id === cell.imageId);
      if (imageAsset && !images[imageAsset.id]) {
        images[imageAsset.id] = imageAsset.url;
      }
      return {
        row,
        col,
        imageId: cell.imageId,
        objectFit: cell.objectFit || 'cover',
        alignment: cell.alignment || 'center',
        rotation: cell.rotation || 0,
        outline: cell.outline || false,
      };
    }).filter(c => images[c.imageId]);

    if (cells.length === 0) {
      setPrintStatus({ type: 'error', message: 'No photos placed on the grid' });
      setTimeout(() => setPrintStatus(null), 3000);
      return;
    }

    setIsPrinting(true);
    setPrintStatus(null);

    try {
      const result = await invoke<string>('cmd_print_direct', {
        job: {
          grid: {
            rows: state.grid.rows,
            cols: state.grid.cols,
            cellWidth: state.grid.cellWidth,
            cellHeight: state.grid.cellHeight,
            gapX: state.grid.gapX,
            gapY: state.grid.gapY,
            paddingTop: state.grid.paddingTop,
            paddingLeft: state.grid.paddingLeft,
            pageWidth: state.grid.pageWidth || 210,
            pageHeight: state.grid.pageHeight || 297,
            dpi: state.grid.dpi || 600,
          },
          images,
          cells,
        },
        printerName: selectedPrinter || null,
      });
      setPrintStatus({ type: 'success', message: result });
    } catch (err: any) {
      if (String(err) === 'Print cancelled') return;
      setPrintStatus({ type: 'error', message: String(err) });
    } finally {
      setIsPrinting(false);
      setTimeout(() => setPrintStatus(null), 5000);
    }
  };

  // Check if layout goes out of bounds
  const checkOverflow = () => {
    const g = state.grid;
    const contentW = g.paddingLeft + (g.cols * g.cellWidth) + (Math.max(0, g.cols - 1) * g.gapX);
    const contentH = g.paddingTop + (g.rows * g.cellHeight) + (Math.max(0, g.rows - 1) * g.gapY);
    return contentW > (g.pageWidth || 210) || contentH > (g.pageHeight || 297);
  };

  useEffect(() => {
    const area = previewAreaRef.current;
    if (!area) return;

    const updateScale = () => {
      const mmToPx = 96 / 25.4;
      const pageWidthPx = (state.grid.pageWidth || 210) * mmToPx;
      const pageHeightPx = (state.grid.pageHeight || 297) * mmToPx;
      const styles = window.getComputedStyle(area);
      const availableWidth = area.clientWidth - parseFloat(styles.paddingLeft) - parseFloat(styles.paddingRight);
      const availableHeight = area.clientHeight - parseFloat(styles.paddingTop) - parseFloat(styles.paddingBottom);
      const nextScale = Math.min(
        1,
        availableWidth / pageWidthPx,
        availableHeight / pageHeightPx
      );

      setPreviewScale(Math.max(0.1, nextScale));
    };

    updateScale();
    const resizeObserver = new ResizeObserver(updateScale);
    resizeObserver.observe(area);
    return () => resizeObserver.disconnect();
  }, [state.grid.pageWidth, state.grid.pageHeight]);

  const pageWidthMm = state.grid.pageWidth || 210;
  const pageHeightMm = state.grid.pageHeight || 297;
  const mmToPx = 96 / 25.4;
  const previewWidthPx = pageWidthMm * mmToPx;
  const previewHeightPx = pageHeightMm * mmToPx;

  return (
    <div className="flex flex-col h-screen bg-slate-50 font-sans overflow-hidden select-none text-slate-800">
      
      {/* Top Toolbar */}
      <header className="h-16 bg-white border-b border-slate-200 flex items-center justify-between px-6 shrink-0 print:hidden z-10">
        <div className="flex items-center gap-3">
          <div className="bg-blue-600 p-2 rounded-lg">
            <Printer className="w-5 h-5 text-white" />
          </div>
          <h1 className="text-xl font-bold tracking-tight text-slate-900">
            PrintCatalog
          </h1>
        </div>

        <div className="flex items-center gap-4">
          <div className="px-4 py-2 border border-slate-200 rounded-lg flex items-center gap-6 bg-slate-50">
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold text-slate-500 uppercase tracking-wider">Style</span>
               <span className="text-sm font-semibold text-slate-800">{state.grid.pageName || 'A4'} ({state.grid.pageWidth || 210} x {state.grid.pageHeight || 297} mm)</span>
            </div>
            <div className="w-px h-4 bg-slate-300" />
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold text-slate-500 uppercase tracking-wider">Matrix</span>
               <span className="text-sm font-semibold text-slate-800">{state.grid.rows} × {state.grid.cols}</span>
            </div>
          </div>
          
          <button 
            onClick={() => {
              setDraftGrid({...state.grid});
              setIsPaperStyleOpen(true);
            }}
            className="flex items-center gap-2 px-5 py-2.5 bg-white border border-slate-200 hover:border-slate-300 hover:bg-slate-50 rounded-lg text-sm font-semibold text-slate-700 transition shadow-sm"
          >
            <LayoutGrid className="w-4 h-4 text-blue-600" /> Paper Style
          </button>

          {/* Printer selector */}
          <select
            value={selectedPrinter}
            onChange={e => setSelectedPrinter(e.target.value)}
            className="px-3 py-2.5 border border-slate-200 rounded-lg text-sm font-medium text-slate-700 bg-white focus:outline-none focus:ring-2 focus:ring-blue-500/20 focus:border-blue-500 max-w-[200px] truncate"
          >
            {printers.length === 0 && <option value="">No printers found</option>}
            {printers.map(p => (
              <option key={p.name} value={p.name}>
                {p.name}{p.is_default ? ' (Default)' : ''}
              </option>
            ))}
          </select>
          
          <button 
            onClick={handlePrint}
            disabled={isPrinting}
            className={`flex items-center gap-2 px-6 py-2.5 rounded-lg text-sm font-semibold text-white transition shadow-sm ${
              isPrinting ? 'bg-blue-400 cursor-not-allowed' : 'bg-blue-600 hover:bg-blue-700'
            }`}
          >
            {isPrinting ? (
              <><Loader2 className="w-4 h-4 animate-spin" /> Printing...</>
            ) : (
              <><Printer className="w-4 h-4" /> Print...</>
            )}
          </button>
        </div>
      </header>

      {/* Main Workspace */}
      <div className="flex-1 flex overflow-hidden">
        
        {/* Left Sidebar - Photo Catalog */}
        <aside className="w-80 bg-white border-r border-slate-200 flex flex-col shrink-0 print:hidden z-10">
          <div className="p-4 border-b border-slate-100 flex justify-between items-center">
             <div className="text-sm font-semibold text-slate-800">Photo Catalog</div>
             <button 
                onClick={() => fileInputRef.current?.click()}
                className="text-blue-600 border border-blue-200 hover:bg-blue-50 text-xs px-3 py-1.5 font-medium rounded-md transition"
             >
               Add Photos
             </button>
             <input type="file" multiple accept="image/*" className="hidden" ref={fileInputRef} onChange={handleImageUpload} />
          </div>

          <div className="p-4 border-b border-slate-100 space-y-4">
            <div>
              <div className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">Apply Range</div>
              <div className="grid grid-cols-3 gap-2">
                {([
                  { value: 'cell' as ApplyRange, label: 'Cell', icon: Square },
                  { value: 'row' as ApplyRange, label: 'Row', icon: Rows3 },
                  { value: 'after' as ApplyRange, label: 'After', icon: ListEnd },
                ]).map(option => {
                  const Icon = option.icon;
                  const active = applyRange === option.value;
                  return (
                    <button
                      key={option.value}
                      type="button"
                      onClick={() => setApplyRange(option.value)}
                      title={option.label}
                      className={`h-11 rounded-lg border flex items-center justify-center transition ${
                        active
                          ? 'bg-blue-50 border-blue-500 text-blue-700 shadow-sm'
                          : 'bg-white border-slate-200 text-slate-500 hover:bg-slate-50 hover:border-slate-300'
                      }`}
                    >
                      <Icon className="w-5 h-5" />
                    </button>
                  );
                })}
              </div>
            </div>

            <div>
              <div className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">Action</div>
              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => setGridAction('place')}
                  className={`h-10 rounded-lg border flex items-center justify-center gap-2 text-sm font-semibold transition ${
                    gridAction === 'place'
                      ? 'bg-blue-600 border-blue-600 text-white shadow-sm'
                      : 'bg-white border-slate-200 text-slate-600 hover:bg-slate-50 hover:border-slate-300'
                  }`}
                >
                  <MousePointer2 className="w-4 h-4" /> Place
                </button>
                <button
                  type="button"
                  onClick={() => setGridAction('clear')}
                  className={`h-10 rounded-lg border flex items-center justify-center gap-2 text-sm font-semibold transition ${
                    gridAction === 'clear'
                      ? 'bg-red-600 border-red-600 text-white shadow-sm'
                      : 'bg-white border-slate-200 text-slate-600 hover:bg-slate-50 hover:border-slate-300'
                  }`}
                >
                  <Eraser className="w-4 h-4" /> Clear
                </button>
              </div>
            </div>
          </div>
          
          <div className="flex-1 overflow-y-auto p-4 space-y-4">
             {state.images.length === 0 && (
               <div className="text-center p-8 text-slate-400 text-sm font-medium">
                 No photos loaded.<br/>Click "Add Photos" to start.
               </div>
             )}
             <div className="grid grid-cols-2 gap-3">
               {state.images.map(img => {
                 const isSelected = state.selectedImageId === img.id;
                 return (
                   <div 
                     key={img.id} 
                     onClick={() => {
                       setState(prev => ({ ...prev, selectedImageId: isSelected ? null : img.id }));
                       if (!isSelected) setGridAction('place');
                     }}
                     className={`
                       relative bg-white rounded-lg p-1.5 transition-all cursor-pointer group flex flex-col
                       ${isSelected ? 'ring-2 ring-blue-500 shadow-md bg-blue-50/10' : 'ring-1 ring-slate-200 hover:ring-slate-300 hover:shadow-sm'}
                     `}
                   >
                     {isSelected && (
                       <div className="absolute -top-2 -right-2 bg-blue-500 text-white rounded-full p-1 shadow-md z-10">
                         <Check className="w-3 h-3" />
                       </div>
                     )}
                     <div className="relative aspect-square w-full bg-slate-50 rounded-md overflow-hidden border border-slate-100">
                       <img src={img.url} className="w-full h-full object-cover" />
                     </div>
                     <div className="mt-2 flex items-center justify-between px-1">
                       <button 
                         onClick={(e) => { e.stopPropagation(); setEditingPhotoId(img.id); }}
                         className="inline-flex items-center gap-1.5 rounded-md bg-blue-50 px-2.5 py-1.5 text-[11px] font-bold uppercase tracking-wide text-blue-700 hover:bg-blue-600 hover:text-white transition"
                       >
                         <Settings className="w-3 h-3" /> Options
                       </button>
                       <button 
                         onClick={(e) => deleteImage(img.id, e)}
                         className="p-1 text-slate-400 hover:text-red-500 rounded transition"
                       >
                         <Trash2 className="w-3.5 h-3.5" />
                       </button>
                     </div>
                   </div>
                 );
               })}
             </div>
          </div>
          <div className="p-4 bg-slate-50 border-t border-slate-200 text-xs text-slate-500 font-medium">
            {gridAction === 'clear'
              ? 'Clear mode removes photos using the selected range.'
              : 'Select a photo, then click the grid using the selected range. Click a placed photo without a selected photo to edit options.'}
          </div>
        </aside>

        {/* Canvas Area */}
        <main ref={previewAreaRef} className="flex-1 overflow-auto p-8 bg-slate-100/50 print:p-0 print:bg-white relative custom-scrollbar">
          <div className="min-h-full min-w-full flex items-center justify-center print:block">
            <div
              className="preview-scale-shell transition-all duration-300 print:w-auto print:h-auto"
              style={{
                width: `${previewWidthPx * previewScale}px`,
                height: `${previewHeightPx * previewScale}px`,
              }}
            >
          <div 
            className="preview-page bg-white shadow-xl relative transition-all duration-300 print:m-0 print:shadow-none mx-auto border border-slate-200 rounded-sm"
            style={{ 
              width: `${pageWidthMm}mm`, 
              height: `${pageHeightMm}mm`,
              transform: `scale(${previewScale})`,
              transformOrigin: 'top left',
            }}
          >
            <div 
              style={{ 
                paddingTop: `${state.grid.paddingTop}mm`, 
                paddingLeft: `${state.grid.paddingLeft}mm` 
              }}
            >
              <div 
                style={{
                  display: 'grid',
                  gridTemplateRows: `repeat(${state.grid.rows}, ${state.grid.cellHeight}mm)`,
                  gridTemplateColumns: `repeat(${state.grid.cols}, ${state.grid.cellWidth}mm)`,
                  gap: `${state.grid.gapY}mm ${state.grid.gapX}mm`,
                }}
              >
                {Array.from({ length: state.grid.rows * state.grid.cols }).map((_, i) => {
                  const r = Math.floor(i / state.grid.cols);
                  const c = i % state.grid.cols;
                  const key = `${r}-${c}`;
                  const cell = state.cells[key];
                  const image = cell ? state.images.find(img => img.id === cell.imageId) : null;

                  return (
                    <div 
                      key={key}
                      onClick={() => stampCell(key)}
                      className={`
                        relative overflow-hidden cursor-pointer border
                        ${gridAction === 'clear' && image ? 'border-slate-400 hover:border-red-500 hover:ring-2 hover:ring-red-200 print:border-transparent' :
                          (!image ? 'border-slate-400 border-dashed hover:bg-blue-50/50 print:border-transparent' : 
                            (cell?.outline ? 'border-black' : 'border-slate-400 hover:border-blue-500 print:border-transparent'))}
                      `}
                      style={{
                        width: `${state.grid.cellWidth}mm`,
                        height: `${state.grid.cellHeight}mm`,
                      }}
                    >
                      {image && (
                        <div className="w-full h-full relative bg-white" style={{ overflow: 'hidden' }}>
                          <img 
                            src={image.url} 
                            className="absolute pointer-events-none"
                            style={{
                              width: '100%',
                              height: '100%',
                              objectFit: cell.objectFit || 'cover',
                              objectPosition: ALIGNMENT_MAP[cell.alignment || 'center'],
                              transform: `rotate(${cell.rotation || 0}deg)`,
                            }}
                          />
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
            </div>
          </div>

          {/* Print status toast */}
          <AnimatePresence>
            {printStatus && (
              <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: 20 }}
                className={`fixed bottom-6 right-6 px-5 py-3 rounded-lg shadow-lg text-sm font-medium z-50 ${
                  printStatus.type === 'success'
                    ? 'bg-emerald-600 text-white'
                    : 'bg-red-600 text-white'
                }`}
              >
                {printStatus.type === 'success' ? (
                  <span className="flex items-center gap-2"><Check className="w-4 h-4" /> {printStatus.message}</span>
                ) : (
                  <span className="flex items-center gap-2"><AlertCircle className="w-4 h-4" /> {printStatus.message}</span>
                )}
              </motion.div>
            )}
          </AnimatePresence>
        </main>
      </div>

      {/* --- Modals --- */}
      
      {/* 1. Paper Style Modal */}
      {isPaperStyleOpen && draftGrid && (
        <div className="fixed inset-0 bg-slate-900/40 backdrop-blur-sm z-50 flex items-center justify-center p-4 print:hidden">
          <motion.div 
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            className="bg-white rounded-xl shadow-2xl w-full max-w-4xl flex flex-col overflow-hidden max-h-[95vh] overflow-y-auto"
          >
            <div className="px-6 py-4 border-b border-slate-100 flex justify-between items-center bg-slate-50/50 sticky top-0 z-10">
               <h2 className="font-semibold text-lg text-slate-800">Paper Style Configuration</h2>
               <button onClick={() => {
                 setPendingTemplateGrid(null);
                 setTemplateNameDraft('');
                 setTemplateNameError('');
                 setIsPaperStyleOpen(false);
               }} className="text-slate-400 hover:text-slate-600 transition"><X className="w-5 h-5" /></button>
            </div>
            
            <div className="p-6 grid grid-cols-1 md:grid-cols-3 gap-8">
               {/* Preview Side */}
               <div className={`col-span-1 p-6 md:p-8 flex flex-col items-center justify-center rounded-xl transition-colors ${
                 (() => {
                   const contentW = draftGrid.paddingLeft + (draftGrid.cols * draftGrid.cellWidth) + (Math.max(0, draftGrid.cols - 1) * draftGrid.gapX);
                   const contentH = draftGrid.paddingTop + (draftGrid.rows * draftGrid.cellHeight) + (Math.max(0, draftGrid.rows - 1) * draftGrid.gapY);
                   return contentW > (draftGrid.pageWidth || 210) || contentH > (draftGrid.pageHeight || 297);
                 })() ? 'bg-red-50' : 'bg-emerald-50'
               }`}>
                  <div 
                    className={`bg-white shadow-sm border transition-colors flex relative ${
                      (() => {
                        const contentW = draftGrid.paddingLeft + (draftGrid.cols * draftGrid.cellWidth) + (Math.max(0, draftGrid.cols - 1) * draftGrid.gapX);
                        const contentH = draftGrid.paddingTop + (draftGrid.rows * draftGrid.cellHeight) + (Math.max(0, draftGrid.rows - 1) * draftGrid.gapY);
                        return contentW > (draftGrid.pageWidth || 210) || contentH > (draftGrid.pageHeight || 297);
                      })() ? 'border-red-400' : 'border-emerald-400'
                    }`}
                    style={{
                      width: '100%',
                      aspectRatio: `${draftGrid.pageWidth || 210} / ${draftGrid.pageHeight || 297}`,
                      containerType: 'size', // Key for accurate internal %
                      overflow: 'hidden'
                    }}
                  >
                     <div 
                        style={{
                           position: 'absolute',
                           top: `${(draftGrid.paddingTop / (draftGrid.pageHeight || 297)) * 100}cqh`,
                           left: `${(draftGrid.paddingLeft / (draftGrid.pageWidth || 210)) * 100}cqw`,
                           display: 'grid',
                           gridTemplateRows: `repeat(${draftGrid.rows}, ${(draftGrid.cellHeight / (draftGrid.pageHeight || 297)) * 100}cqh)`,
                           gridTemplateColumns: `repeat(${draftGrid.cols}, ${(draftGrid.cellWidth / (draftGrid.pageWidth || 210)) * 100}cqw)`,
                           rowGap: `${(draftGrid.gapY / (draftGrid.pageHeight || 297)) * 100}cqh`,
                           columnGap: `${(draftGrid.gapX / (draftGrid.pageWidth || 210)) * 100}cqw`,
                        }}
                     >
                        {Array.from({ length: draftGrid.rows * draftGrid.cols }).map((_, i) => (
                          <div key={i} className="bg-slate-200 border border-slate-300"></div>
                        ))}
                     </div>
                  </div>
                  {(() => {
                     const contentW = draftGrid.paddingLeft + (draftGrid.cols * draftGrid.cellWidth) + (Math.max(0, draftGrid.cols - 1) * draftGrid.gapX);
                     const contentH = draftGrid.paddingTop + (draftGrid.rows * draftGrid.cellHeight) + (Math.max(0, draftGrid.rows - 1) * draftGrid.gapY);
                     const isOverflowing = contentW > (draftGrid.pageWidth || 210) || contentH > (draftGrid.pageHeight || 297);
                     return isOverflowing ? (
                      <div className="mt-4 flex items-center gap-1.5 text-red-600 font-medium text-xs">
                         <AlertCircle className="w-4 h-4" /> Layout exceeds bounds
                      </div>
                    ) : (
                      <div className="mt-4 flex items-center gap-1.5 text-emerald-600 font-medium text-xs">
                         <Check className="w-4 h-4" /> Layout fits cleanly
                      </div>
                    );
                  })()}
               </div>
               
               {/* Controls Side */}
               <div className="col-span-2 space-y-6">
                  
                  <div className="mb-6">
                    <label className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2 block">Style Template</label>
                    <select 
                      className="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-sm font-medium text-slate-700 focus:outline-none focus:ring-2 focus:ring-blue-500/20 focus:border-blue-500 transition"
                      value={(BUILT_IN_TEMPLATES.some(t => t.id === draftGrid.id) && !state.savedTemplates.some(template => gridsMatch(template, draftGrid))) ? 'custom' : draftGrid.id}
                      onChange={e => {
                        const selectedTemplate = state.savedTemplates.find(template => template.id === e.target.value);
                        if (selectedTemplate) setDraftGrid({ ...selectedTemplate });
                      }}
                    >
                       {(BUILT_IN_TEMPLATES.some(t => t.id === draftGrid.id) && !state.savedTemplates.some(template => gridsMatch(template, draftGrid))) && (
                         <option value="custom">Unsaved changes</option>
                       )}
                       {state.savedTemplates.map(template => (
                         <option key={template.id} value={template.id}>
                           {template.name} ({template.pageWidth || 210} x {template.pageHeight || 297} mm)
                         </option>
                       ))}
                    </select>
                  </div>

                  <div className="grid grid-cols-2 gap-6">
                    {/* Paper Group */}
                    <div>
                      <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Paper Constraints</h3>
                      <div className="space-y-3 bg-slate-50 p-4 rounded-lg border border-slate-100">
                         <div className="flex justify-between items-center text-sm">
                            <span className="text-slate-600 font-medium">Page Width</span>
                            <div className="flex items-center gap-2">
                              <DraftInput value={draftGrid.pageWidth || 210} onChange={v => setDraftGrid(p => p ? ({...p, pageName: 'Custom', pageWidth: Math.max(1, v)}) : p)} /> mm
                            </div>
                         </div>
                         <div className="flex justify-between items-center text-sm">
                            <span className="text-slate-600 font-medium">Page Height</span>
                            <div className="flex items-center gap-2">
                              <DraftInput value={draftGrid.pageHeight || 297} onChange={v => setDraftGrid(p => p ? ({...p, pageName: 'Custom', pageHeight: Math.max(1, v)}) : p)} /> mm
                            </div>
                         </div>
                         <div className="flex justify-between items-center text-sm pt-2 border-t border-slate-200">
                            <span className="text-slate-600 font-medium">Top Margin</span>
                            <div className="flex items-center gap-2">
                               <DraftInput value={draftGrid.paddingTop} onChange={v => setDraftGrid(p => p ? ({...p, paddingTop: v}) : p)} /> mm
                            </div>
                         </div>
                         <div className="flex justify-between items-center text-sm">
                            <span className="text-slate-600 font-medium">Left Margin</span>
                            <div className="flex items-center gap-2">
                               <DraftInput value={draftGrid.paddingLeft} onChange={v => setDraftGrid(p => p ? ({...p, paddingLeft: v}) : p)} /> mm
                            </div>
                         </div>
                      </div>
                    </div>
                    
                    <div className="space-y-6">
                      {/* Layout Group */}
                      <div>
                        <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Grid Layout</h3>
                        <div className="space-y-3 bg-slate-50 p-4 rounded-lg border border-slate-100">
                           <div className="flex justify-between items-center text-sm">
                              <span className="text-slate-600 font-medium">Rows</span>
                              <DraftInput value={draftGrid.rows} onChange={v => setDraftGrid(p => p ? ({...p, rows: Math.max(1, v)}) : p)} />
                           </div>
                           <div className="flex justify-between items-center text-sm">
                              <span className="text-slate-600 font-medium">Columns</span>
                              <DraftInput value={draftGrid.cols} onChange={v => setDraftGrid(p => p ? ({...p, cols: Math.max(1, v)}) : p)} />
                           </div>
                           <div className="flex justify-between items-center text-sm pt-2 border-t border-slate-200">
                              <span className="text-slate-600 font-medium">Row Gap</span>
                              <div className="flex items-center gap-2">
                                 <DraftInput value={draftGrid.gapY} onChange={v => setDraftGrid(p => p ? ({...p, gapY: v}) : p)} /> mm
                              </div>
                           </div>
                           <div className="flex justify-between items-center text-sm">
                              <span className="text-slate-600 font-medium">Column Gap</span>
                              <div className="flex items-center gap-2">
                                 <DraftInput value={draftGrid.gapX} onChange={v => setDraftGrid(p => p ? ({...p, gapX: v}) : p)} /> mm
                              </div>
                           </div>
                        </div>
                      </div>
                      
                      {/* Cell Group */}
                      <div>
                        <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Cell Dimensions</h3>
                        <div className="space-y-3 bg-slate-50 p-4 rounded-lg border border-slate-100">
                           <div className="flex justify-between items-center text-sm">
                              <span className="text-slate-600 font-medium">Width</span>
                              <div className="flex items-center gap-2">
                                 <DraftInput value={draftGrid.cellWidth} onChange={v => setDraftGrid(p => p ? ({...p, cellWidth: v}) : p)} /> mm
                              </div>
                           </div>
                           <div className="flex justify-between items-center text-sm">
                              <span className="text-slate-600 font-medium">Height</span>
                              <div className="flex items-center gap-2">
                                 <DraftInput value={draftGrid.cellHeight} onChange={v => setDraftGrid(p => p ? ({...p, cellHeight: v}) : p)} /> mm
                              </div>
                           </div>
                        </div>
                      </div>
                    </div>
                  </div>
               </div>
            </div>
            
            <div className="bg-slate-50 px-6 py-4 flex justify-end gap-3 border-t border-slate-100 sticky bottom-0 z-10">
               <button 
                  onClick={applyPaperStyle} 
                  className="px-6 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition shadow-sm"
               >
                  Save & Apply
               </button>
            </div>
          </motion.div>
        </div>
      )}

      {pendingTemplateGrid && (
        <div className="fixed inset-0 bg-slate-900/50 backdrop-blur-sm z-[60] flex items-center justify-center p-4 print:hidden">
          <motion.div
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: 1, scale: 1 }}
            className="bg-white rounded-xl shadow-2xl w-full max-w-md overflow-hidden"
          >
            <div className="px-6 py-4 border-b border-slate-100 flex items-center justify-between bg-slate-50/50">
              <h2 className="font-semibold text-lg text-slate-800">Save Paper Style</h2>
              <button
                onClick={() => {
                  setPendingTemplateGrid(null);
                  setTemplateNameDraft('');
                  setTemplateNameError('');
                }}
                className="text-slate-400 hover:text-slate-600 transition"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <div className="p-6 space-y-4">
              <div>
                <label className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2 block">
                  Paper Style Name
                </label>
                <input
                  autoFocus
                  value={templateNameDraft}
                  onChange={e => {
                    setTemplateNameDraft(e.target.value);
                    setTemplateNameError('');
                  }}
                  onKeyDown={e => {
                    if (e.key === 'Enter') saveNamedPaperStyle();
                  }}
                  className={`w-full px-3 py-2.5 rounded-lg border text-sm font-medium outline-none transition ${
                    templateNameError
                      ? 'border-red-300 focus:border-red-500 focus:ring-2 focus:ring-red-500/10'
                      : 'border-slate-200 focus:border-blue-500 focus:ring-2 focus:ring-blue-500/20'
                  }`}
                />
                {templateNameError && (
                  <div className="mt-2 flex items-center gap-1.5 text-xs font-medium text-red-600">
                    <AlertCircle className="w-4 h-4" /> {templateNameError}
                  </div>
                )}
              </div>
            </div>

            <div className="bg-slate-50 px-6 py-4 flex justify-end gap-3 border-t border-slate-100">
              <button
                onClick={() => {
                  setPendingTemplateGrid(null);
                  setTemplateNameDraft('');
                  setTemplateNameError('');
                }}
                className="px-5 py-2 bg-white border border-slate-200 hover:bg-slate-50 rounded-lg text-sm font-semibold text-slate-700 transition"
              >
                Cancel
              </button>
              <button
                onClick={saveNamedPaperStyle}
                className="px-5 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm font-semibold transition shadow-sm"
              >
                Save & Apply
              </button>
            </div>
          </motion.div>
        </div>
      )}

      {/* 2. Photo Options Modal */}
      {editingPhotoId && (
        <div className="fixed inset-0 bg-slate-900/40 backdrop-blur-sm z-50 flex items-center justify-center p-4 print:hidden">
          <motion.div 
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            className="bg-white rounded-xl shadow-2xl w-full max-w-4xl flex flex-col overflow-hidden"
          >
            <div className="px-6 py-4 border-b border-slate-100 flex justify-between items-center bg-slate-50/50">
               <h2 className="font-semibold text-lg text-slate-800">Photo Options & Layout</h2>
               <button onClick={() => setEditingPhotoId(null)} className="text-slate-400 hover:text-slate-600 transition"><X className="w-5 h-5" /></button>
            </div>
            
            <div className="p-6 grid grid-cols-1 md:grid-cols-2 gap-8">
               {/* Preview Side */}
               <div className="bg-slate-50 border border-slate-200 rounded-xl p-6 flex flex-col items-center justify-center min-h-[400px]">
                 <div 
                   className="overflow-hidden bg-white shadow-md relative"
                   style={{
                     aspectRatio: `${state.grid.cellWidth} / ${state.grid.cellHeight}`,
                     width: state.grid.cellWidth >= state.grid.cellHeight ? '100%' : 'auto',
                     height: state.grid.cellHeight > state.grid.cellWidth ? '100%' : 'auto',
                     maxHeight: '350px',
                     maxWidth: '100%',
                     border: state.imageConfigs[editingPhotoId]?.outline ? '1px solid black' : '1px solid #cbd5e1'
                   }}
                 >
                   {(() => {
                     const imgAsset = state.images.find(img => img.id === editingPhotoId);
                     const config = state.imageConfigs[editingPhotoId] || { objectFit: 'cover', alignment: 'center', rotation: 0, outline: false };
                     return imgAsset ? (
                       <img 
                          src={imgAsset.url} 
                          className="w-full h-full"
                          style={{
                            objectFit: config.objectFit,
                            objectPosition: ALIGNMENT_MAP[config.alignment || 'center'],
                            transform: `rotate(${config.rotation || 0}deg)`,
                          }}
                       />
                     ) : null;
                   })()}
                 </div>
                 <div className="mt-6 text-sm text-slate-500 font-medium">Cell Preview</div>
               </div>
               
               {/* Controls Side */}
               <div className="space-y-6 py-2 h-[450px] overflow-y-auto pr-2 custom-scrollbar">
                 
                 <div>
                    <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Size in Cell</h3>
                    <div className="space-y-2 bg-slate-50 rounded-lg p-4 border border-slate-100">
                       {(() => {
                         const config = state.imageConfigs[editingPhotoId] || {};
                         const updateConf = (update: any) => setState(prev => {
                           const newConf = { ...prev.imageConfigs[editingPhotoId], ...update };
                           const newCells = { ...prev.cells };
                           Object.keys(newCells).forEach(k => {
                              if (newCells[k].imageId === editingPhotoId) newCells[k] = { ...newCells[k], ...update };
                           });
                           return { ...prev, imageConfigs: { ...prev.imageConfigs, [editingPhotoId]: newConf }, cells: newCells };
                         });
                         return (
                           <>
                             <div 
                               className="flex items-center gap-3 cursor-pointer p-2 rounded hover:bg-slate-100 transition" 
                               onClick={() => updateConf({objectFit: 'contain'})}
                             >
                               <div className={`flex flex-col items-center justify-center w-4 h-4 rounded-full border ${config.objectFit === 'contain' ? 'border-blue-500' : 'border-slate-300'}`}>
                                  {config.objectFit === 'contain' && <div className="w-2 h-2 bg-blue-500 rounded-full" />}
                               </div>
                               <span className="text-sm font-medium text-slate-700">Fit the Cell (No Trimming)</span>
                             </div>
                             <div 
                               className="flex items-center gap-3 cursor-pointer p-2 rounded hover:bg-slate-100 transition" 
                               onClick={() => updateConf({objectFit: 'cover'})}
                             >
                               <div className={`flex flex-col items-center justify-center w-4 h-4 rounded-full border ${config.objectFit !== 'contain' ? 'border-blue-500' : 'border-slate-300'}`}>
                                  {config.objectFit !== 'contain' && <div className="w-2 h-2 bg-blue-500 rounded-full" />}
                               </div>
                               <span className="text-sm font-medium text-slate-700">Fit the Cell (Trimming)</span>
                             </div>
                           </>
                         )
                       })()}
                    </div>
                 </div>

                 <div>
                    <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Cell Options</h3>
                    <div className="bg-slate-50 rounded-lg p-4 border border-slate-100 flex items-center">
                       <label className="flex items-center gap-3 cursor-pointer">
                         <input 
                           type="checkbox" 
                           className="w-4 h-4 text-blue-600 rounded border-slate-300 focus:ring-blue-500"
                           checked={state.imageConfigs[editingPhotoId]?.outline || false}
                           onChange={e => {
                             const updateConf = { outline: e.target.checked };
                             setState(prev => {
                               const newConf = { ...prev.imageConfigs[editingPhotoId], ...updateConf };
                               const newCells = { ...prev.cells };
                               Object.keys(newCells).forEach(k => {
                                  if (newCells[k].imageId === editingPhotoId) newCells[k] = { ...newCells[k], outline: newConf.outline };
                               });
                               return { ...prev, imageConfigs: { ...prev.imageConfigs, [editingPhotoId]: newConf }, cells: newCells };
                             });
                           }}
                         />
                         <span className="text-sm font-medium text-slate-700">Cell Outline</span>
                       </label>
                    </div>
                 </div>

                 <div>
                    <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Alignment in Cell</h3>
                    <div className="flex gap-6 items-center bg-slate-50 rounded-lg p-4 border border-slate-100">
                       <div className="grid grid-cols-3 w-24 h-24 border border-slate-200 bg-white rounded overflow-hidden shadow-sm">
                          {['top-left', 'top-center', 'top-right', 'center-left', 'center', 'center-right', 'bottom-left', 'bottom-center', 'bottom-right'].map(pos => {
                             const config = state.imageConfigs[editingPhotoId] || {};
                             const isActive = (config.alignment || 'center') === pos;
                             return (
                               <div 
                                 key={pos} 
                                 onClick={() => {
                                   setState(prev => {
                                     const newConf = { ...prev.imageConfigs[editingPhotoId], alignment: pos };
                                     const newCells = { ...prev.cells };
                                     Object.keys(newCells).forEach(k => {
                                        if (newCells[k].imageId === editingPhotoId) newCells[k] = { ...newCells[k], alignment: pos };
                                     });
                                     return { ...prev, imageConfigs: { ...prev.imageConfigs, [editingPhotoId]: newConf }, cells: newCells };
                                   })
                                 }}
                                 className={`border border-slate-100 cursor-pointer transition-colors hover:bg-blue-50 ${isActive ? 'bg-blue-500' : 'bg-transparent'}`}
                               />
                             )
                          })}
                       </div>
                       <span className="text-sm text-slate-500 font-medium">Click a grid sector to snap alignment</span>
                    </div>
                 </div>

                 <div>
                    <h3 className="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Orientation</h3>
                    <div className="flex gap-3 bg-slate-50 rounded-lg p-4 border border-slate-100">
                       <button 
                         onClick={() => {
                           setState(prev => {
                             const rot = ((prev.imageConfigs[editingPhotoId]?.rotation || 0) - 90) % 360;
                             const newConf = { ...prev.imageConfigs[editingPhotoId], rotation: rot };
                             const newCells = { ...prev.cells };
                             Object.keys(newCells).forEach(k => {
                                if (newCells[k].imageId === editingPhotoId) newCells[k] = { ...newCells[k], rotation: rot };
                             });
                             return { ...prev, imageConfigs: { ...prev.imageConfigs, [editingPhotoId]: newConf }, cells: newCells };
                           });
                         }}
                         className="flex items-center gap-2 px-4 py-2 bg-white border border-slate-200 hover:border-blue-300 hover:text-blue-600 rounded-lg shadow-sm transition text-slate-600 focus:outline-none focus:ring-2 focus:ring-blue-500"
                       >
                         <RotateCcw className="w-4 h-4" /> 
                         <span className="text-sm font-medium">Rotate Left</span>
                       </button>
                       <button 
                         onClick={() => {
                           setState(prev => {
                             const rot = ((prev.imageConfigs[editingPhotoId]?.rotation || 0) + 90) % 360;
                             const newConf = { ...prev.imageConfigs[editingPhotoId], rotation: rot };
                             const newCells = { ...prev.cells };
                             Object.keys(newCells).forEach(k => {
                                if (newCells[k].imageId === editingPhotoId) newCells[k] = { ...newCells[k], rotation: rot };
                             });
                             return { ...prev, imageConfigs: { ...prev.imageConfigs, [editingPhotoId]: newConf }, cells: newCells };
                           });
                         }}
                         className="flex items-center gap-2 px-4 py-2 bg-white border border-slate-200 hover:border-blue-300 hover:text-blue-600 rounded-lg shadow-sm transition text-slate-600 focus:outline-none focus:ring-2 focus:ring-blue-500"
                       >
                         <RotateCw className="w-4 h-4" />
                         <span className="text-sm font-medium">Rotate Right</span>
                       </button>
                    </div>
                 </div>
                 
               </div>
            </div>

            <div className="bg-slate-50 px-6 py-4 flex justify-end gap-3 border-t border-slate-100">
               <button onClick={() => setEditingPhotoId(null)} className="px-6 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition shadow-sm">Done</button>
            </div>
          </motion.div>
        </div>
      )}
    </div>
  );
}
