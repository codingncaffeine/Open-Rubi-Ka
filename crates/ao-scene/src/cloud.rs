//! DisplaySystem VisualSingleCloud: initialize100532b8, grid10053681,
//! respawn10053e45, update1005485a, rasterize10054e07.
use crate::{Mesh, Vertex};

pub const COUNT: usize = 30;
pub const GRID: usize = 10;
#[allow(clippy::approx_constant)]
const NATIVE_PI: f32 = 3.14; // DisplaySystem double1008c380 stores this f32 value.

#[derive(Clone, Debug, PartialEq)]
pub struct CloudLayer {
    pub mesh: usize,
    pub altitude: f32,
    /// AO coordinates (left handed), native high-altitude wind.
    pub wind: [f32; 3],
    pub storm: f32,
    pub thick: f32,
    /// AO GameSunDirection.y.
    pub sun_y: f32,
    /// GroundLightCurrent (gamma space) used by10054e07, not CloudLightCurrent.
    pub light: [f32; 3],
    /// Direction towards the ground light in scene coordinates.
    pub light_direction: [f32; 3],
    pub ambient: [f32; 3],
}

/// EnvPrefs15 wins over16 unless the native camera disables realistic clouds.
pub fn resolution(realistic: bool, simple: bool, disabled_realistic: bool) -> usize {
    if realistic && !disabled_realistic {10} else if simple {2} else {0}
}

#[derive(Clone, Debug)]
struct Sheet {
    position: [f32; 3],
    size: [f32; 2],
    speed: f32,
    opacity: f32,
    vertices: [Vertex; GRID * GRID],
}
#[derive(Clone, Debug)]
pub struct Clouds {
    sheets: Vec<Sheet>,
    seed: u32,
    grid: usize,
    previous_eye: Option<[f32; 3]>,
}
impl Default for Clouds {
    fn default() -> Self { Self::new(1) }
}
impl Clouds {
    pub fn new(seed: u32) -> Self {
        let mut out=Self {sheets:Vec::with_capacity(COUNT),seed,grid:0,previous_eye:None};
        for _ in 0..COUNT {
            let speed=out.random()*0.5+0.6;
            let size=out.size();
            out.sheets.push(Sheet {position:[0.0;3],size,speed,opacity:1.0,vertices:[Vertex::default();GRID*GRID]});
        }
        out
    }
    // MSVCRT rand (the imported client CRT), not a platform-dependent Rust generator.
    fn word(&mut self)->u32 {self.seed=self.seed.wrapping_mul(214013).wrapping_add(2531011);(self.seed>>16)&32767}
    fn random(&mut self)->f32 {(self.word()%10000) as f32/10000.0}
    fn size(&mut self)->[f32;2] {
        let r=self.random();let scale=(1.0-r*r)*5.0+1.0;
        [(self.random()*2.0+1.0)*100.0*scale,(self.random()*2.0+1.0)*100.0*scale]
    }
    fn shape(&mut self, i:usize, respawn:bool) {
        let n=self.grid;
        let mut warp=[1.0;40];
        if n>2 {
            for side in 0..4 {
                let frequency=(self.word()&3)+1;
                let amplitude=0.1+self.random()*0.1;
                for k in 0..n {warp[side*n+k]=((k as f32/(n-1) as f32)*frequency as f32*NATIVE_PI).sin()*amplitude+1.0;}
            }
        }
        if respawn {self.sheets[i].size=self.size();}
        let size=self.sheets[i].size;
        for y in 0..n {for x in 0..n {
            let u=x as f32/(n-1) as f32;let v=y as f32/(n-1) as f32;
            let px=u-0.5;let pz=v-0.5;
            let normal=if n>2 {unit([(self.random()*2.0-1.0)*8.0,self.random()*0.4+0.01,(self.random()*2.0-1.0)*8.0])} else {[0.0,1.0,0.0]};
            self.sheets[i].vertices[y*n+x]=Vertex {pos:[px*size[0]*warp[if px>=0.0 {y}else{n+y}],0.0,pz*size[1]*warp[if pz>=0.0 {2*n+x}else{3*n+x}]],normal,uv:[u*0.99+0.005,v*0.99+0.005],color:[170.0/255.0;4]};
        }}
        self.sheets[i].opacity=(self.random()*2.0-1.0)*0.1+1.0;
    }
    fn respawn(&mut self,i:usize,all:bool,altitude:f32) {
        self.shape(i,true);
        let mut position=self.sheets[i].position;
        let mut random_x=all;let mut random_z=all;
        if !all {
            if position[0]<=-1600.0 {position[0]=1500.0;random_z=true;} else if position[0]>=1600.0 {position[0]=-1500.0;random_z=true;}
            if position[2]<=-1600.0 {position[2]=1500.0;random_x=true;random_z=false;} else if position[2]>=1600.0 {position[2]=-1500.0;random_x=true;random_z=false;}
        }
        position[1]=altitude;
        for _ in 0..10 {
            if random_x {position[0]=(self.random()*2.0-1.0)*1600.0-200.0;}
            if random_z {position[2]=(self.random()*2.0-1.0)*1600.0-200.0;}
            let separate=self.sheets.iter().enumerate().all(|(j,s)| j==i || ((!random_x || (position[0].abs()-s.position[0].abs()).abs()>=50.0) && (!random_z || (position[2].abs()-s.position[2].abs()).abs()>=50.0)));
            if separate {break;}
        }
        self.sheets[i].position=position;
    }
    /// Updates preallocated high-detail storage. Simple sheets occupy the first four
    /// vertices of each100-vertex slot; indices switch only when preferences change.
    pub fn update(&mut self,layer:&CloudLayer,grid:usize,eye:[f32;3],dt:f32,far:f32,vertices:&mut[Vertex]) {
        if grid==0 {self.grid=0;return;}
        let changed=self.grid!=grid;
        self.grid=grid;
        if changed {for i in 0..COUNT {self.shape(i,false);}}
        let delta=self.previous_eye.map_or([0.0;3],|old|[old[0]-eye[0],0.0,eye[2]-old[2]]);
        let moved=self.previous_eye.is_none() || (delta[0]*delta[0]+delta[2]*delta[2]).sqrt()>800.0;
        self.previous_eye=Some(eye);
        for i in 0..COUNT {
            let speed=self.sheets[i].speed;
            let p=&mut self.sheets[i].position;
            p[0]+=layer.wind[0]*speed*dt*100.0+delta[0]*10.0;
            p[1]+=layer.wind[1]*speed*dt*100.0;
            p[2]+=layer.wind[2]*speed*dt*100.0+delta[2]*10.0;
            if p[0]<=-1600.0 || p[0]>=1600.0 || p[2]<=-1600.0 || p[2]>=1600.0 || moved {self.respawn(i,moved,layer.altitude);}
            let s=&self.sheets[i];
            let distance=((s.position[0]*s.position[0]+(s.position[1]-layer.altitude).powi(2)+s.position[2]*s.position[2]).sqrt()/1500.0).min(1.0).powi(2);
            let angle=distance*0.5;
            let axis=unit([s.position[2],0.0,-s.position[0]]);
            let center=[s.position[0],s.position[1]+layer.altitude*(-1.5)*distance,s.position[2]];
            let grey=((layer.sun_y+0.89).clamp(0.14,1.0)-0.14)*(1.0-layer.thick*0.9)+0.14;
            let grey=(s.opacity*grey).min(1.0);
            let grey=(grey*255.0).trunc()/255.0;
            let material_sheet=if i<2 {0}else if i<4 {2}else if i<27 {4}else{27};
            let emissive=((self.sheets[material_sheet].opacity*(((layer.sun_y+0.89).clamp(0.14,1.0)-0.14)*(1.0-layer.thick*0.9)+0.14)).min(1.0)*255.0).trunc()/255.0;
            let alpha=((1.0-layer.storm*layer.storm)*255.0).trunc()/255.0;
            for k in 0..grid*grid {
                let source=s.vertices[k];
                let p=rotate(source.pos,axis,angle);let normal=rotate(source.normal,axis,angle);
                let normal=[normal[0],normal[1],-normal[2]];
                let diffuse=dot(normal,layer.light_direction).max(0.0)*0.55;
                let rgb=if grid>2 {std::array::from_fn(|j|linear((emissive+layer.ambient[j]+grey*layer.light[j]*diffuse).min(1.0)))}else{[linear(grey);3]};
                vertices[i*GRID*GRID+k]=Vertex {pos:[(p[0]+center[0])*far/1000.0,(p[1]+center[1])*far/1000.0,-(p[2]+center[2])*far/1000.0],normal,uv:source.uv,color:[rgb[0],rgb[1],rgb[2],alpha]};
            }
        }
    }
    pub fn mesh()->Mesh {Mesh {vertices:vec![Vertex::default();COUNT*GRID*GRID],..Mesh::default()}}
    pub fn indices(grid:usize,first:usize,end:usize)->Vec<u32> {
        let mut out=Vec::with_capacity((end-first)*(grid-1)*(grid-1)*6);
        for i in first..end {for y in 0..grid-1 {for x in 0..grid-1 {
            let a=(i*GRID*GRID+y*grid+x) as u32;
            out.extend([a,a+grid as u32,a+1,a+1,a+grid as u32,a+grid as u32+1]);
        }}}
        out
    }
}
fn dot(a:[f32;3],b:[f32;3])->f32 {a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
fn unit(v:[f32;3])->[f32;3] {let n=dot(v,v).sqrt();if n==0.0 {[0.0,1.0,0.0]}else{v.map(|x|x/n)}}
fn rotate(v:[f32;3],a:[f32;3],angle:f32)->[f32;3] {let (s,c)=angle.sin_cos();let d=dot(a,v)*(1.0-c);[v[0]*c+(a[1]*v[2]-a[2]*v[1])*s+a[0]*d,v[1]*c+(a[2]*v[0]-a[0]*v[2])*s+a[1]*d,v[2]*c+(a[0]*v[1]-a[1]*v[0])*s+a[2]*d]}
fn linear(v:f32)->f32 {if v<=0.04045 {v/12.92}else{((v+0.055)/1.055).powf(2.4)}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_cloud_modes_and_topology() {
        assert_eq!(resolution(true,true,false),10);assert_eq!(resolution(false,true,false),2);
        assert_eq!(resolution(true,true,true),2);assert_eq!(resolution(true,false,true),0);
        assert_eq!(resolution(false,false,false),0);
        let layer=CloudLayer {mesh:0,altitude:100.0,wind:[0.001,0.0,0.002],storm:0.0,thick:0.0,sun_y:1.0,light:[1.0;3],light_direction:[0.0,1.0,0.0],ambient:[0.0;3]};
        let mut sim=Clouds::default();let mut mesh=Clouds::mesh();
        assert_eq!(sim.sheets.len(),30);
        assert!((sim.sheets[0].speed-0.60205).abs()<0.000001);
        for n in [10,2,0,10] {
            sim.update(&layer,n,[0.0;3],0.016,1000.0,&mut mesh.vertices);
            if n!=0 {let indices=Clouds::indices(n,0,COUNT);assert_eq!(indices.len(),COUNT*(n-1)*(n-1)*6);assert!(indices.iter().all(|&i|(i as usize)<mesh.vertices.len()));}
            assert!(mesh.vertices.iter().all(|v|v.pos.iter().chain(v.color.iter()).all(|x|x.is_finite())));
            if n==2 {assert_eq!(sim.sheets[0].vertices[0].normal,[0.0,1.0,0.0]);}
        }
        sim.previous_eye=Some([0.0;3]);
        for sheet in &mut sim.sheets {sheet.position=[100.0,100.0,100.0];}
        let old=sim.sheets[0].position;
        let speed=sim.sheets[0].speed;
        let mut windy=layer.clone();windy.wind=[1.0,0.0,0.0];windy.storm=1.0;
        sim.update(&windy,10,[1.0,0.0,0.0],0.01,1000.0,&mut mesh.vertices);
        assert!((sim.sheets[0].position[0]-(old[0]+speed-10.0)).abs()<0.00001);
        assert_eq!(mesh.vertices[0].color[3],0.0);
    }
}
